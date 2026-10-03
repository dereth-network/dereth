//! Wield and wear legality, automatic equipment selection,
//! and inventory sorting.
//!
//! Every one of these is a *legality precheck*: it decides whether to show a message and whether to
//! send a request. None applies a move; moves are applied when the server confirms them.

use super::slots::{loc, SlotSide};
use super::SplitState;
use crate::weenie::item_type;

/// Whether the object in the ready slot rules
/// out a shield. Retail makes three checks:
///
/// A missile weapon (combat-use value 2) blocks a shield when its ammo type is nonzero.
/// A two-handed weapon (combat-use value 5) also blocks it, as does an item with
/// the caster type bit (bit 15). These checks use the public item description and
/// item type, without reading the occupied equipment location.
///
/// The location is not read. Testing `location & TWO_HANDED` instead would answer *no* for a
/// bow and a wand and *yes* for a two-hander only after it has been wielded — none of which is
/// what the function computes.
#[must_use]
pub fn blocks_use_of_shield(w: &crate::Weenie) -> bool {
    use crate::combat::combat_use;
    let cu = w.pwd.combat_use.unwrap_or(combat_use::NONE);
    if cu == combat_use::MISSILE && w.pwd.ammo_type.unwrap_or(0) != 0 {
        return true;
    }
    if cu == combat_use::TWO_HANDED {
        return true;
    }
    w.inq_type() & item_type::CASTER != 0
}
use crate::world::World;
use crate::{NoticeSink, RequestSink};
use dereth_primitives::{ObjectId, ServerTime};

/// The "unwield the thing in the way, then wield mine" retry state.
///
/// Dropping onto an occupied paper-doll slot is not refused silently: the client moves the
/// blocker to the backpack and retries the requested equipment operation after confirmation.
/// The state below preserves the blocked item, destination and chosen slot for that retry.
///
/// The four call sites, all now modelled:
///
/// | site | what it does |
/// |---|---|
/// | Before slot search | clear the blocker, blocked item, selected side and attempt count |
/// | When unblocking is allowed | increment the count, record the blocked item/side, announce the backpack move and request placement |
/// | Blocker-move reply | save the retry inputs, clear retry state, then retry only if the destination matches |
/// | Rejected blocker move | unghost the blocked item and clear retry state |
#[derive(Debug, Clone, Copy, Default)]
pub struct UnblockState {
    /// The blocker in **the last occupied slot found**, not the first;
    /// the equipment planner updates this while walking the slots.
    pub blocking_id: Option<ObjectId>,
    /// The item the player was trying to wield.
    pub blocked_id: Option<ObjectId>,
    /// Where the blocker was sent, and the destination its reply must match.
    pub blocking_dest_id: Option<ObjectId>,
    /// The requested equipment side, so the retry aims at the same wrist or
    /// ring the player did.
    pub blocked_side: SlotSide,
    /// The spell-component half of the retry state; this build has no writer for it,
    /// and resetting the equipment retry does not clear it either.
    pub blocked_spell_id: u32,
    /// The unblocking attempt count; zero disables retry processing.
    pub unblock_attempt_num: u32,
}

impl UnblockState {
    /// Clear the four fields that enable and identify an equipment retry.
    ///
    /// The original reset clears the blocker, blocked item, side and attempt count, while
    /// retaining the destination, spell id and spell target. This Rust record has no spell
    /// target field; its destination and spell id likewise remain unchanged. Resetting the
    /// entire record would differ: a stale destination is harmless because a zero attempt
    /// count prevents retry processing from reading it.
    pub fn reset(&mut self) {
        self.blocking_id = None;
        self.blocked_id = None;
        self.blocked_side = SlotSide::Null;
        self.unblock_attempt_num = 0;
    }
}

/// The client's **single-bit** slot walk and each slot's "already
/// wearing" message.
///
/// Verified against retail. Three things about this list are easy to get wrong and every one
/// of them is invisible from the outside:
///
/// * **The neck comes first**, not fifteenth. The wield's first slot test after the legality
///   check is the neck bit `0x8000`; the trinket (`0x04000000`) is second.
/// * **The strings are retail's wide literals**, transcribed here verbatim ("chest armor", not
///   "a chest piece").
/// * **The four paired/ready locations are not part of this walk at all.** `WRIST_WEAR`,
///   `FINGER_WEAR`, `WEAPON_READY_SLOT`, `SHIELD` and `MISSILE_AMMO` are handled by their own
///   code, which does **not** send the table's mask:
///   the paired slots send the side's own bit and the ready group sends
///   `valid_locations & 0x03500000`. Putting them in a "walk the bits" list would
///   answer `0x03500000` for a bow whose whole `valid_locations`
///   is `0x00400000` — a value the corpus's own `0x001A`s contradict five times over.
pub const WIELD_SLOT_ORDER: [(u32, &str); 15] = [
    (
        loc::NECK_WEAR,
        "You're already wearing jewelry on your neck.",
    ),
    (loc::TRINKET_ONE, "You're already wearing a trinket."),
    (loc::CLOAK, "You're already wearing a cloak."),
    (loc::SIGIL_ONE, "You're already using aetheria of Lyr."),
    (loc::SIGIL_TWO, "You're already using aetheria of Kor."),
    (loc::SIGIL_THREE, "You're already using aetheria of Tem."),
    (loc::HEAD_WEAR, "You're already wearing a helm."),
    (loc::CHEST_ARMOR, "You're already wearing chest armor."),
    (loc::ABDOMEN_ARMOR, "You're already wearing abdomen armor."),
    (
        loc::UPPER_ARM_ARMOR,
        "You're already wearing upper arm armor.",
    ),
    (
        loc::LOWER_ARM_ARMOR,
        "You're already wearing lower arm armor.",
    ),
    (loc::HAND_WEAR, "You're already wearing hand armor."),
    (
        loc::UPPER_LEG_ARMOR,
        "You're already wearing upper leg armor.",
    ),
    (
        loc::LOWER_LEG_ARMOR,
        "You're already wearing lower leg armor.",
    ),
    (loc::FOOT_WEAR, "You're already wearing foot armor."),
];

/// Both wrist slots occupied.
pub const WRIST_BLOCKED_MESSAGE: &str = "You're already wearing jewelry on both wrists.";
/// Both finger slots occupied. The client says "hands", not "fingers".
pub const FINGER_BLOCKED_MESSAGE: &str = "You're already wearing jewelry on both hands.";
/// The shirt-and-pants pair.
pub const SHIRT_AND_PANTS_BLOCKED_MESSAGE: &str =
    "You must unwield your shirt and pants to wield that.";
/// *"You cannot wield more %s"*, formatted with **the item being wielded**'s
/// `NAME_PLURAL` (name type 1).
///
/// This is **not** the ready group's general refusal. The
/// literal is used **once** in the whole of the wield, inside the
/// *ammunition* slot's same-weenie-class arm — the one arm that shows a string and then declines
/// to set the blocking id — and it is guarded by `quiet`, not by `unblock`. Every other
/// ready-group block is silent. The request-free planner only preserves whether the
/// ammunition arm replaces the walk's prior blocker.
pub const READY_SLOT_BLOCKED_MESSAGE: &str = "You cannot wield more";

/// `CLOTHING_LOC`'s shirt-and-pants pair, the one composite the wield itself ever sends
/// (`0x42`).
pub const SHIRT_AND_PANTS: u32 = loc::CHEST_WEAR | loc::UPPER_LEG_WEAR;

/// The outcome of a wield attempt before anything is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WieldPlan {
    /// A free slot was found; wield to exactly this bit.
    Wield(u32),
    /// Every candidate slot is occupied.
    ///
    /// **`slot` and `blocker` are the walk's *last* blocked slot, not the first.**
    /// The wield walk stores the blocking id at **every** blocked slot as it goes and the
    /// write is *before* the check that guards the message, so the blocking id — the
    /// id the unblock retry unwields — is the last one examined.
    ///
    /// `messages` is every blocked slot's string in walk order, because the client displays each
    /// one as it passes (`quiet == 0`) rather than picking one. `messages.last()` is `slot`'s.
    Blocked {
        slot: u32,
        blocker: Option<ObjectId>,
        messages: Vec<&'static str>,
    },
    /// The item is not wieldable at all.
    NotWieldable,
}

/// The request produced by an equipment drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquipmentDropOutcome {
    Refused,
    Wield,
    Wear,
    Unblock,
}

impl World {
    /// Quiet slot and canvas feedback, evaluated against the same inventory as equipment requests.
    #[must_use]
    pub fn equipment_hover(&self, item: ObjectId) -> dereth_client_contract::view::EquipmentHover {
        let slot_mask = self.weenie(item).map(|w| {
            if self.auto_wield_is_legal(item).is_err() {
                return 0;
            }
            let valid = w.pwd.valid_locations.unwrap_or(0);
            if valid & loc::MELEE_WEAPON != 0 {
                valid | loc::SHIELD
            } else {
                valid
            }
        });
        let canvas = match self.auto_wear_is_legal(item) {
            Ok(()) => Some(true),
            Err((_, true)) => None,
            Err((_, false)) => Some(false),
        };
        dereth_client_contract::view::EquipmentHover { slot_mask, canvas }
    }

    /// Equip at the interface's resolved location. Clothing wears using the full item mask;
    /// other slots wield on the requested side and may first move a blocker to the backpack.
    #[allow(clippy::too_many_arguments)]
    pub fn drop_equipment_at_location(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        mask: u32,
        side: u32,
        split: SplitState,
        now: ServerTime,
    ) -> EquipmentDropOutcome {
        let Some(w) = self.weenie(item) else {
            return EquipmentDropOutcome::Refused;
        };
        let mut valid = w.pwd.valid_locations.unwrap_or(0);
        let side = if mask == loc::SHIELD && valid & loc::MELEE_WEAPON != 0 {
            valid |= loc::SHIELD;
            SlotSide::Left
        } else {
            match side {
                1 => SlotSide::Left,
                2 => SlotSide::Right,
                _ => SlotSide::Null,
            }
        };
        if mask & valid == 0 {
            return EquipmentDropOutcome::Refused;
        }
        if mask & loc::CLOTHING != 0 {
            if self.auto_wear(req, out, item, split, now, false).is_ok() {
                EquipmentDropOutcome::Wear
            } else {
                EquipmentDropOutcome::Refused
            }
        } else if self.auto_wield(req, out, item, side, false, true, false, split, now) {
            if self.unblock.unblock_attempt_num > 0 {
                EquipmentDropOutcome::Unblock
            } else {
                EquipmentDropOutcome::Wield
            }
        } else {
            EquipmentDropOutcome::Refused
        }
    }

    /// Drop on the figure itself, asking the server to choose from all wearable locations.
    pub fn drop_equipment_on_canvas(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        split: SplitState,
        now: ServerTime,
    ) -> EquipmentDropOutcome {
        let Some(w) = self.weenie(item) else {
            return EquipmentDropOutcome::Refused;
        };
        if w.pwd.valid_locations.unwrap_or(0) & loc::WEARABLE == 0 {
            self.refuse(
                out,
                false,
                dereth_client_contract::panels::inventory::CANNOT_PUT_THAT_ITEM_THERE,
            );
            return EquipmentDropOutcome::Refused;
        }
        if self.auto_wear(req, out, item, split, now, false).is_ok() {
            EquipmentDropOutcome::Wear
        } else {
            EquipmentDropOutcome::Refused
        }
    }

    /// Check whether automatic wearing is legal and report whether the item is already worn:
    /// `(id, already_worn, quiet)`.
    ///
    /// **Checked against retail**, for example with a worn shirt dropped back on its
    /// own doll slot. The two arms are not the same string:
    ///
    /// ```text
    ///   worn = 0
    ///   ready_for_inventory_request(0)                ; the literal 0, not quiet
    ///   valid_locations & 0x8007FFF == 0      -> false
    ///   priority & clothing_priority_mask == 0 -> true
    ///   the item is on the player              -> worn = 1
    ///   quiet != 0                             -> false, silent
    ///   worn:  "The %s is already being worn", name(item, 2)
    ///   else:  blocker = the object at (inventory_mask & valid, priority)
    ///          no blocker weenie -> false, silent
    ///          "You must remove your %s to wear that", name(blocker, 2)
    ///   display on 0x1A; false
    /// ```
    ///
    /// `NameType` 2 is `NAME_APPROPRIATE` in both arms. The display is the caller's here
    /// ([`Self::auto_wear`] passes its `quiet`), which is why the string comes back rather than
    /// going out.
    ///
    /// # Errors
    /// `(message, already_worn)`. The message is empty for the silent refusals: not ready, no
    /// weenie, not wearable, and a blocker the client has no object for.
    pub fn auto_wear_is_legal(&self, id: ObjectId) -> Result<(), (String, bool)> {
        // Quiet is the literal 0 — the check speaks for itself in the client whatever the
        // caller's `quiet`; this build's version returns the string and leaves the display to
        // `auto_wear`, which is the one deviation left here.
        if let Err(e) = self.ready_for_inventory_request(false) {
            return Err((e.to_string(), false));
        }
        let Some(w) = self.weenie(id) else {
            return Err((String::new(), false));
        };
        let valid = w.pwd.valid_locations.unwrap_or(0);
        if valid & loc::WEARABLE == 0 {
            return Err((String::new(), false));
        }
        let priority = w.pwd.priority.unwrap_or(0);
        if priority & self.clothing_priority_mask == 0 {
            return Ok(()); // nothing in the way
        }
        let inv = self.player.and_then(|p| self.inventory(p));
        // It may be *this* item, already worn.
        if inv.is_some_and(|inv| inv.location_on_object(id) != 0) {
            let name = w.display_name(
                crate::weenie::NameType::Appropriate,
                self.material_name(w.pwd.material_type.unwrap_or(0)),
            );
            return Err((format!("The {name} is already being worn"), true));
        }
        // The blocker, and a blocker with no weenie is a silent false.
        let Some(blocker) = inv
            .and_then(|inv| inv.object_at_location(self.inventory_mask & valid, priority))
            .and_then(|b| self.weenie(b))
        else {
            return Err((String::new(), false));
        };
        let name = blocker.display_name(
            crate::weenie::NameType::Appropriate,
            self.material_name(blocker.pwd.material_type.unwrap_or(0)),
        );
        Err((format!("You must remove your {name} to wear that"), false))
    }

    /// Check whether automatic wielding is legal for `id`, optionally suppressing feedback.
    ///
    /// **Checked against retail**; its visible answer is the cursor's red/green hint. The
    /// shield blocker is `blocks_use_of_shield`, not "the ready weapon's location is
    /// two-handed", and the strings are retail's. Retail's order and formats:
    ///
    /// ```text
    ///   no weenie for id -> false
    ///   valid = valid_locations; valid & 0x7fffffff == 0 -> false
    ///   ready = the item in the weapon-ready slot
    ///   valid & 0x800000 (MISSILE_AMMO): ready exists
    ///           && ready.ammo_type != 0 && ready.ammo_type != item.ammo_type ->
    ///           quiet ? false : sprintf("Cannot be used with %s", ready.name(2))
    ///   valid & 0x200000 (SHIELD): ready exists && blocks_use_of_shield(ready) ->
    ///           quiet ? false : sprintf("A shield may not be worn with the %s", ready.name(0))
    ///   valid & 0x1000000 (HELD): combat mode == NONCOMBAT -> true;
    ///           type & 0x8000 (CASTER) -> true;
    ///           quiet ? false : sprintf("Cannot hold %s while in combat", item.name(2))
    ///   true
    /// ```
    ///
    /// Every refusal string goes out through the local-feedback notice
    /// on chat type `0x1A`. `NameType` 2 is `NAME_APPROPRIATE`, 0 is `NAME_SINGULAR`.
    ///
    /// # Errors
    /// The refusal string, in the order the client tests the three cases. Empty for the two
    /// silent early-outs.
    pub fn auto_wield_is_legal(&self, id: ObjectId) -> Result<(), String> {
        let Some(w) = self.weenie(id) else {
            return Err(String::new());
        };
        let valid = w.pwd.valid_locations.unwrap_or(0);
        if valid & loc::ALL == 0 {
            return Err(String::new());
        }
        let ready = self.inv_slots.weapon_ready();

        if valid & loc::MISSILE_AMMO != 0 {
            if let Some(r) = self.weenie(ready) {
                let launcher_ammo = r.pwd.ammo_type.unwrap_or(0);
                if launcher_ammo != 0 && launcher_ammo != w.pwd.ammo_type.unwrap_or(0) {
                    let name = r.display_name(
                        crate::weenie::NameType::Appropriate,
                        self.material_name(r.pwd.material_type.unwrap_or(0)),
                    );
                    return Err(format!("Cannot be used with {name}"));
                }
            }
        }
        if valid & loc::SHIELD != 0 {
            if let Some(r) = self.weenie(ready) {
                if blocks_use_of_shield(r) {
                    let name = r.display_name(
                        crate::weenie::NameType::Singular,
                        self.material_name(r.pwd.material_type.unwrap_or(0)),
                    );
                    return Err(format!("A shield may not be worn with the {name}"));
                }
            }
        }
        if valid & loc::HELD != 0 {
            if self.combat.combat_mode == crate::combat::CombatMode::NonCombat {
                return Ok(());
            }
            // Tested as bit 15 of the *type*, which is `TYPE_CASTER`.
            if w.inq_type() & item_type::CASTER != 0 {
                return Ok(());
            }
            let name = w.display_name(
                crate::weenie::NameType::Appropriate,
                self.material_name(w.pwd.material_type.unwrap_or(0)),
            );
            return Err(format!("Cannot hold {name} while in combat"));
        }
        Ok(())
    }

    /// The client's slot walk, without sending anything.
    ///
    /// **The answer is the slot the wield request is sent for**, so getting it wrong is invisible:
    /// the item equips, just not where the player aimed. Transcribed step by step
    /// from the whole of the wield walk, in the four groups the function actually has:
    ///
    /// 1. **The fifteen single-bit worn slots**, [`WIELD_SLOT_ORDER`], neck first. Each free bit is
    ///    sent as the constant itself.
    /// 2. **The paired wrist/finger slots**. `valid_locations & 0x00030000` selects
    ///    the wrists, else `& 0x000C0000` the fingers; `side` then picks the left
    ///    or right slot record and **its** own location is what is sent — `0x00010000` rather than
    ///    `0x00030000`. Note the client tests either bit, not both: a right-wrist-only item dropped
    ///    with `side = LEFT` is still aimed at the left wrist.
    /// 3. **The ready group**. First the off-hand rewrite —
    ///    `if ((valid & 0x02500000) && side == LEFT) valid = 0x00200000;` — which is how
    ///    the client's "melee weapon dropped on the shield slot"
    ///    becomes an off-hand wield. Then `valid & 0x03500000` (**the intersection**, not the
    ///    slot constant), then the shield bit, then the ammunition bit.
    /// 4. **The shirt/pants/cloak group**, the one place the wield sends a
    ///    composite: shirt and pants both valid and both free are sent together as
    ///    [`SHIRT_AND_PANTS`] (`0x42`).
    ///
    /// **The blocker is the *last* slot, not the first.** The store
    /// `blocking_id = <the slot's item id>` runs at every blocked slot in the walk and it runs
    /// **before** the quiet check that skips the message, so the last write wins and
    /// `blocking_id` names the last occupied slot the walk touched. The retry unwields exactly
    /// that item, so an item valid for both the trinket and the head with both occupied takes
    /// your **helm** off, not your trinket.
    /// The messages are all of them, in order, because the client prints each as it passes.
    ///
    /// `other_side`: when the chosen wrist or ring is
    /// occupied by something that is not this item, `other_side != 0` lets the walk try the *other*
    /// side of the pair before giving up. `auto_sort` and the unblock retry pass `1`; a paper-doll
    /// drop passes `0`, because the player aimed at one slot.
    #[must_use]
    pub fn plan_auto_wield(&self, id: ObjectId, side: SlotSide, other_side: bool) -> WieldPlan {
        let Some(w) = self.weenie(id) else {
            return WieldPlan::NotWieldable;
        };
        let mut valid = w.pwd.valid_locations.unwrap_or(0);
        if valid & loc::ALL == 0 {
            return WieldPlan::NotWieldable;
        }
        let inv = self.inventory_mask;
        let mut walk = WieldWalk::default();

        // 1. The fifteen single-bit worn slots.
        for (bit, message) in WIELD_SLOT_ORDER {
            if valid & bit == 0 {
                continue;
            }
            if inv & bit == 0 {
                return WieldPlan::Wield(bit);
            }
            walk.block(bit, Some(message));
        }

        // 2. The paired wrist and finger slots — `side` names the slot record.
        let paired = if valid & loc::WRIST_WEAR != 0 {
            Some((loc::WRIST_WEAR, WRIST_BLOCKED_MESSAGE))
        } else if valid & loc::FINGER_WEAR != 0 {
            Some((loc::FINGER_WEAR, FINGER_BLOCKED_MESSAGE))
        } else {
            None
        };
        if let Some((pair, message)) = paired {
            let bit = paired_side_slot(pair, side);
            if inv & bit == 0 {
                return WieldPlan::Wield(bit);
            }
            // `other_side` and "the chosen slot is not already holding *this* item" open
            // the other half of the pair. Without `other_side` the walk gives up on the side the
            // player aimed at, which is what a paper-doll drop wants.
            let other = paired_side_slot(pair, opposite_side(side));
            if other_side && self.inv_slots.item_at(bit) != Some(id) && inv & other == 0 {
                return WieldPlan::Wield(other);
            }
            walk.block(bit, Some(message));
        }

        // 3. The ready group, and the off-hand rewrite that precedes it.
        //
        // **The whole of the ready group, checked against retail.** A bare
        // `if (inventory_mask & 0x3500000) == 0 { wield }` is three of the arm's tests
        // short, and the one that matters most: with a **shield** worn and the main hand empty,
        // `inventory_mask & 0x3500000` is zero — `SHIELD` is `0x00200000` and the ready mask is
        // `0x03500000` — so a bow would go straight onto the wire as a lone `0x001A` and the shard
        // would refuse it (ACE `Player_Inventory.cs`, `CheckWeaponCollision`, `case
        // EquipMask.MissileWeapon`: *"offhand != null && item.IsAmmoLauncher"* -> a save failure).
        // Retail's client never sends that request: it unwields the shield first.
        //
        // ```text
        //   ready = valid & WEAPON_READY_SLOT
        //   if ready == 0, skip the ready-slot handling
        //   if inventory_mask & 0x3500000 != 0 -> the occupied arm (below)
        //   shield = the item in the shield slot
        //   if shield && the **item being wielded** blocks shield use:
        //       blocking_id = the shield; skip the wield
        //   else:
        //       if the item has an ammo type, the ammo slot is filled with a known object,
        //       and its ammo type differs: blocking_id = the ammunition
        //       if blocking_id == 0: **wield to `ready`**; anything blocked -> do not wield
        //   occupied arm:
        //       try to merge id into the main-hand item; merged -> true, nothing else sent
        //       otherwise blocking_id = the main-hand item
        // ```
        //
        // Three readings that are easy to get backwards:
        //
        // * **`blocks_use_of_shield` is asked about the item being wielded, not about the shield.**
        //   It is the weenie looked up for `id` at the head of the function. So the test is "does *this* rule a shield out" — a bow
        //   with an ammo type, a two-handed weapon, or a caster ([`blocks_use_of_shield`]) — and
        //   a one-handed sword dropped next to a shield is untouched.
        // * **The `blocking_id == 0` gate is the walk's, not this arm's.** A slot
        //   blocked back in group 1 or 2 suppresses the ready-slot wield as well, which is why it
        //   is read off `walk` rather than off the two tests just above it.
        // * **No message.** Every arm here writes `blocking_id` and nothing else; the only string
        //   the wield has for the ready group is *"You cannot wield more %s"*,
        //   used once, inside the **ammunition** slot's same-weenie-class arm, and
        //   guarded by `quiet` rather than by `unblock`. Emitting `READY_SLOT_BLOCKED_MESSAGE`
        //   at the three blocked arms here would be an invention.
        if valid & loc::WEAPON != 0 && side == SlotSide::Left {
            valid = loc::SHIELD;
        }
        let ready = valid & loc::WEAPON_READY_SLOT;
        if ready != 0 {
            if inv & loc::WEAPON_READY_SLOT == 0 {
                // The off-hand rules the main hand out, not the other way round.
                let shield = self.inv_slots.item_at(loc::SHIELD);
                if shield.is_some() && blocks_use_of_shield(w) {
                    walk.block(loc::SHIELD, None);
                } else {
                    // The launcher and the ammunition already up must agree.
                    let mine = w.pwd.ammo_type.unwrap_or(0);
                    let up = self.inv_slots.item_at(loc::MISSILE_AMMO);
                    if mine != 0 {
                        if let Some(a) = up.and_then(|a| self.weenie(a)) {
                            if a.pwd.ammo_type.unwrap_or(0) != mine {
                                walk.block(loc::MISSILE_AMMO, None);
                            }
                        }
                    }
                    if walk.last_blocked.is_none() {
                        return WieldPlan::Wield(ready);
                    }
                }
            } else {
                // The main-hand arm first attempts a merge of `id` into the main-hand item. The
                // request-free plan records its failure arm; `auto_wield` interleaves the shared
                // request-bearing merge before consuming it. Retail sends literal opcode `0x0054`.
                walk.block(loc::WEAPON_READY_SLOT, None);
            }
        }
        if valid & loc::SHIELD != 0 {
            if inv & loc::SHIELD == 0 {
                return WieldPlan::Wield(loc::SHIELD);
            }
            // The shield arm records the blocking id silently.
            walk.block(loc::SHIELD, None);
        }
        if valid & loc::MISSILE_AMMO != 0 {
            if inv & loc::MISSILE_AMMO == 0 {
                return WieldPlan::Wield(loc::MISSILE_AMMO);
            }
            // When the merge fails, matching weenie class ids show
            // "You cannot wield more %s" and deliberately do **not** replace `blocking_id`.
            // The request-bearing caller below owns the string; this decision walk owns the
            // fallback blocker, so only an unknown or different class blocks here.
            let mine = w.pwd.wcid;
            let same_wcid = self
                .inv_slots
                .item_at(loc::MISSILE_AMMO)
                .and_then(|up| self.weenie(up))
                .is_some_and(|up| up.pwd.wcid == mine);
            if !same_wcid {
                walk.block(loc::MISSILE_AMMO, None);
            }
        }

        // 4. Shirt, pants and cloak.
        if valid & loc::CLOTHING != 0 {
            let shirt = valid & loc::CHEST_WEAR != 0;
            let pants = valid & loc::UPPER_LEG_WEAR != 0;
            let cloak = valid & loc::CLOAK != 0;
            let shirt_worn = inv & loc::CHEST_WEAR != 0;
            let pants_worn = inv & loc::UPPER_LEG_WEAR != 0;
            let cloak_worn = inv & loc::CLOAK != 0;
            if shirt && pants {
                if !shirt_worn && !pants_worn {
                    return WieldPlan::Wield(SHIRT_AND_PANTS);
                }
                // The one arm that stops the walk instead of falling through to the cloak.
                walk.block(SHIRT_AND_PANTS, Some(SHIRT_AND_PANTS_BLOCKED_MESSAGE));
                return finish_blocked(self, walk.last_blocked, walk.messages);
            } else if shirt && !shirt_worn {
                return WieldPlan::Wield(loc::CHEST_WEAR);
            } else if !shirt && pants && !pants_worn {
                return WieldPlan::Wield(loc::UPPER_LEG_WEAR);
            } else if cloak && !cloak_worn {
                return WieldPlan::Wield(loc::CLOAK);
            }
        }
        finish_blocked(self, walk.last_blocked, walk.messages)
    }

    /// Plan automatic sorting as a decision over `(id, allow_wield, quiet)`.
    ///
    /// The order is exactly the client's: wear if wearable; then wield if allowed and wieldable;
    /// then, only if the item is **not** already owned by the player, place it in a backpack.
    ///
    /// **Its only callers are tests**, and [`Self::auto_sort`] is the real transcription, so this
    /// is a **second, unwired model of the same function**. Kept, because the tests ask the
    /// decision question and answering it without sending is what they need — but the two can
    /// drift, and a change to one belongs in both.
    #[must_use]
    pub fn plan_auto_sort(&self, id: ObjectId, allow_wield: bool) -> AutoSortPlan {
        if self.ready_for_inventory_request(true).is_err() {
            return AutoSortPlan::Refused;
        }
        let Some(w) = self.weenie(id) else {
            return AutoSortPlan::Refused;
        };
        let valid = w.pwd.valid_locations.unwrap_or(0);
        if valid & loc::WEARABLE != 0 {
            match self.auto_wear_is_legal(id) {
                Ok(()) => return AutoSortPlan::Wear(valid),
                // `if (worn) return false;` — an item already worn stops the walk entirely.
                Err((_, true)) => return AutoSortPlan::Refused,
                Err((_, false)) => {}
            }
        }
        if allow_wield && valid & loc::WIELDABLE != 0 {
            // The fallback wields with `(id, SlotSide::Right, quiet = 1, unblock = 0,
            // allow_sort = 0, other_side = 1)`, so it may take the other wrist or ring.
            if let WieldPlan::Wield(bit) = self.plan_auto_wield(id, SlotSide::Right, true) {
                return AutoSortPlan::Wield(bit);
            }
        }
        if !self.is_owned_by_player(id) {
            return AutoSortPlan::PlaceInBackpack;
        }
        AutoSortPlan::Refused
    }

    /// Execute automatic sorting for `id` — **the operation, not merely the plan.**
    ///
    /// [`Self::plan_auto_sort`] above is the decision alone; this is the function, and
    /// `using_item`'s case 4 calls it whole rather than open-coding some of its five steps.
    ///
    /// 1. The inventory-readiness check runs with quiet 0; a refusal answers `false`, as does an
    ///    id with no weenie.
    /// 2. If the valid locations meet `0x08007FFF`, `auto_wear` runs with the caller's `quiet` and
    ///    reports whether the item is already worn: already worn answers `false`, and a
    ///    successful wear answers `true`.
    /// 3. If `allow_wield` is set and the valid locations meet `0x7EFF8000`, a `true` from
    ///    `auto_wield(id, SlotSide::Right, 1, 0, 0, 1)` answers `true`.
    /// 4. An item the player does not own goes to `place_in_backpack(id, false)`.
    /// 5. Otherwise the answer is `false`.
    ///
    /// Three of those five are easy to drop, and each is observable:
    ///
    /// * **`quiet` belongs to `auto_wear`, and `using_item` passes 0.** The automatic-wear step is
    ///   the only thing that ever says *"You must remove your &lt;blocker&gt; to wear that"*, and it
    ///   says it when its third argument is 0. With `quiet = 1` a double-click on
    ///   a chest piece over an occupied chest slot would do nothing **and say nothing**.
    /// * **`worn` stops the walk.** An item you are already wearing must not fall through to the
    ///   wield leg. `auto_wear_is_legal`'s second return value carries it.
    /// * **the two `valid &` gates**, without which a purely wearable item runs the whole wield
    ///   walk and prints its per-slot refusals.
    ///
    /// The wield leg's own arguments are `auto_wield(id, SlotSide::Right, 1, 0, 0, 1)` — quiet,
    /// **no unblock**, other-side on: see [`Self::wield_to_first_free`]. Note that this is exactly
    /// the tuple `using_item`'s cases 3 and 8 do **not** use.
    ///
    /// Returns `auto_sort`'s own `bool`.
    #[allow(clippy::too_many_arguments)]
    pub fn auto_sort(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        id: ObjectId,
        allow_wield: bool,
        quiet: bool,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        // The readiness check with the literal quiet 0, so this
        // head speaks whatever the caller's `quiet` is.
        if let Err(e) = self.ready_for_inventory_request(false) {
            self.refuse(out, false, e);
            return false;
        }
        let Some(w) = self.weenie(id) else {
            return false;
        };
        let valid = w.pwd.valid_locations.unwrap_or(0);
        if valid & loc::WEARABLE != 0 {
            // The legality check answers its `bool` and fills `worn` through an
            // out-parameter; this crate returns the same two things as an `Err` pair. The order
            // below is the client's and it matters: `auto_wear` **runs and displays first**, and
            // `worn` is tested only afterwards — so an item already on the doll still gets its
            // refusal line before the walk stops.
            let worn = matches!(self.auto_wear_is_legal(id), Err((_, true)));
            let ok = self.auto_wear(req, out, id, split, now, quiet).is_ok();
            // Already worn answers `false` — before the `ok` test.
            if worn {
                return false;
            }
            if ok {
                return true;
            }
        }
        if allow_wield
            && valid & loc::WIELDABLE != 0
            && self.wield_to_first_free(req, out, id, SlotSide::Right, split, now)
        {
            return true;
        }
        // The not-owned arm. Unreachable from `using_item`'s case 4, which the use-result
        // classifier only answers inside its own owned-by-player branch — transcribed because
        // `auto_sort` is a whole function and the next caller may not have that guarantee.
        if !self.is_owned_by_player(id) {
            return self.place_in_backpack(req, out, id, false, split, now);
        }
        false
    }

    /// Behavior: `auto_wear_is_legal(id, &worn, quiet)`, then `attempt_wield(w.valid_locations)` with the **whole** mask, letting the
    /// server pick.
    ///
    /// **`split` is the caller's.** Automatic wear does not "pass the whole stack"; it does
    /// not pass a stack at all: the wear entry point passes one argument, the mask, and
    /// the UI wield request reads the item holder's split size and
    /// max split size out of the item-holder globals themselves, sending
    /// `Request::StackableSplitToWield` instead when they differ and the stack is larger than one.
    /// A caller that has a splitter — the paper-doll drop does — must hand its own state over;
    /// a caller that does not passes [`Self::whole_stack_of`], which is what an untouched slider
    /// holds.
    ///
    /// # Errors
    /// The refusal string.
    pub fn auto_wear(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        id: ObjectId,
        split: SplitState,
        now: ServerTime,
        quiet: bool,
    ) -> Result<(), String> {
        // Automatic wear *displays* its refusal when its third
        // argument is 0, and passes the caller's `quiet` straight through; otherwise the one
        // refusal a doll drop onto a full shirt or pants slot is supposed to show — "You must
        // remove your <blocker> to wear that" — would be silent. The message is built by
        // `auto_wear_is_legal` from the blocker's own name (or, for an item already on the doll,
        // "The <item> is already being worn", which is what a worn item dropped back on its own
        // slot says in retail).
        if let Err((m, _)) = self.auto_wear_is_legal(id) {
            if !m.is_empty() {
                self.refuse(out, quiet, &m);
            }
            return Err(m);
        }
        let valid = self
            .weenie(id)
            .and_then(|w| w.pwd.valid_locations)
            .unwrap_or(0);
        self.attempt_wield(req, out, id, valid, split, now, quiet)
            .map_err(ToString::to_string)
    }

    /// Execute automatic wielding with the requested side, feedback, unblock, sorting, and opposite-side options —
    /// **the whole function**, head and tail, not just the slot walk.
    ///
    /// [`Self::plan_auto_wield`] is only the middle of it. The head and the tail are here, and
    /// the tail is the one a player hits constantly:
    ///
    /// * The inventory-readiness check runs with `quiet`; a refusal answers `false`, as does an id
    ///   with no weenie.
    /// * An item wielded by someone other than the player answers `false`, after displaying
    ///   *"The %s is being wielded by someone else"* unless `quiet`.
    /// * Valid locations with nothing under `0x7FFFFFFF` answer `false`.
    /// * If `auto_wield_is_legal(id, quiet)` refuses, the answer is `false` without `allow_sort` or
    ///   when the player owns the item, and `auto_sort(id, 0, 0)` otherwise.
    /// * The blocker, blocked item, blocked side and attempt count are zeroed, then the walk
    ///   (`plan_auto_wield`) runs.
    /// * If the walk found a blocker: without `unblock` the unblock state is reset and the answer
    ///   is `false`, and a blocker that is the item itself answers `false`. Otherwise the attempt
    ///   count is incremented, the blocked item and side are recorded, and the text is *"Moving %s
    ///   to your backpack"* with the blocker's name. If
    ///   `attempt_to_place_in_container(blocking, player, 0, 1, 0)` succeeds, the blocker's
    ///   destination becomes the player and the answer is `true`; if not, *" - cannot unwield the
    ///   %s"* is appended when the blocker's weenie exists, the unblock state is reset, and the
    ///   answer is `false`. The text is displayed on `0x1A` either way.
    ///
    /// **`unblock` is load-bearing in two directions at once**: it is the same
    /// flag that guards all fifteen per-slot *"You're already wearing …"* strings
    /// (once per slot), so the call site that gets the retry
    /// is exactly the call site that shows **no** message. The paper doll's
    /// drag-accept wields with `(id, side, 0, 1, 0, 0)` — quiet off, unblock on, other-side off.
    ///
    /// Returns the wield's own `bool`: `true` when something went on the wire, including when
    /// what went on the wire was the *blocker's* move rather than the wield.
    #[allow(clippy::too_many_arguments)]
    pub fn auto_wield(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        id: ObjectId,
        side: SlotSide,
        quiet: bool,
        unblock: bool,
        other_side: bool,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        if let Err(e) = self.ready_for_inventory_request(quiet) {
            self.refuse(out, quiet, e);
            return false;
        }
        let Some(w) = self.weenie(id) else {
            return false;
        };
        let wielder = w.pwd.wielder_id.unwrap_or_default();
        if wielder.0 != 0 && Some(wielder) != self.player {
            if !quiet {
                let name = w.display_name(
                    crate::weenie::NameType::Appropriate,
                    self.material_name(w.pwd.material_type.unwrap_or(0)),
                );
                out.emit(crate::Notice::DisplayString {
                    channel: super::FEEDBACK_CHANNEL,
                    text: format!("The {name} is being wielded by someone else"),
                });
            }
            return false;
        }
        if w.pwd.valid_locations.unwrap_or(0) & loc::ALL == 0 {
            return false;
        }
        if let Err(text) = self.auto_wield_is_legal(id) {
            // `allow_sort` is 0 at both of this rebuild's call sites, so the
            // `auto_sort` fall-through is not reachable and is not written.
            if !quiet && !text.is_empty() {
                self.refuse(out, quiet, &text);
            }
            return false;
        }
        // The walk starts from a cleared state every time, which is why
        // `unblock_attempt_num` is a flag and not a counter — see [`Self::unblock_on_item_moved`].
        self.unblock.reset();
        let plan = self.plan_auto_wield(id, side, other_side);

        // `auto_wield`'s two calls are interleaved with the otherwise
        // request-free slot walk. A free worn/wrist/finger slot returns before either call; every
        // other outcome reached the ready group. Keep the shared merge machinery as the one
        // implementation of split-size and remaining-room arithmetic.
        if !wield_returns_before_ready(&plan) {
            let mut valid = self
                .weenie(id)
                .map_or(0, |item| item.pwd.valid_locations.unwrap_or(0));
            if valid & loc::WEAPON != 0 && side == SlotSide::Left {
                valid = loc::SHIELD;
            }
            let ready = valid & loc::WEAPON_READY_SLOT;
            if ready != 0 && self.inventory_mask & loc::WEAPON_READY_SLOT != 0 {
                let target = self.inv_slots.weapon_ready();
                if self.item_holder_attempt_merge(req, out, id, target, split, now) {
                    return true;
                }
            }

            // A successful direct ready/shield wield returns before the ammunition arm. All
            // other plans continue to the ammunition arm.
            let returns_before_ammo = matches!(
                plan,
                WieldPlan::Wield(bit)
                    if (ready != 0 && bit == ready) || bit == loc::SHIELD
            );
            if !returns_before_ammo
                && valid & loc::MISSILE_AMMO != 0
                && self.inventory_mask & loc::MISSILE_AMMO != 0
            {
                let target = self
                    .inv_slots
                    .item_at(loc::MISSILE_AMMO)
                    .unwrap_or_default();
                if self.item_holder_attempt_merge(req, out, id, target, split, now) {
                    return true;
                }
                let same_wcid = self
                    .weenie(target)
                    .zip(self.weenie(id))
                    .is_some_and(|(up, item)| up.pwd.wcid == item.pwd.wcid);
                if same_wcid && !quiet {
                    let name = self.weenie(id).map_or_else(String::new, |item| {
                        item.display_name(
                            crate::weenie::NameType::Plural,
                            self.material_name(item.pwd.material_type.unwrap_or(0)),
                        )
                    });
                    out.emit(crate::Notice::DisplayString {
                        channel: super::FEEDBACK_CHANNEL,
                        text: format!("{READY_SLOT_BLOCKED_MESSAGE} {name}"),
                    });
                }
            }
        }

        match plan {
            WieldPlan::Wield(bit) => self
                .attempt_wield(req, out, id, bit, split, now, quiet)
                .is_ok(),
            WieldPlan::NotWieldable => false,
            WieldPlan::Blocked {
                blocker, messages, ..
            } => {
                // The blocking-id store at every blocked slot -- the walk's own
                // record, which is why this is the *last* one.
                self.unblock.blocking_id = blocker;
                if !unblock {
                    // **The per-slot strings are guarded by `unblock`, not by `quiet`**
                    // (once per slot). So `auto_sort`, which passes
                    // `quiet = 1` and `unblock = 0`, still shows them, and the paper doll, which
                    // passes `quiet = 0` and `unblock = 1`, shows none. One argument, two jobs.
                    for m in messages {
                        out.emit(crate::Notice::DisplayString {
                            channel: super::FEEDBACK_CHANNEL,
                            text: m.to_string(),
                        });
                    }
                }
                // No blocker leaves the tail entirely -- no unblock reset,
                // because there is nothing set to reset.
                let Some(blocking) = blocker else {
                    return false;
                };
                if !unblock {
                    // The walk's own reset.
                    self.unblock.reset();
                    return false;
                }
                self.begin_unblock(req, out, id, blocking, side, split, now)
            }
        }
    }

    /// `auto_wield`'s tail — "move the blocker to your backpack and remember to come
    /// back for this".
    ///
    /// Returns `true` when the blocker's move went on the wire. The caller's item is **not** sent
    /// and stays ghosted; [`Self::unblock_on_item_moved`] sends it when the server confirms.
    #[allow(clippy::too_many_arguments)]
    fn begin_unblock(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        blocked: ObjectId,
        blocking: ObjectId,
        side: SlotSide,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        // The thing in the way is the thing being wielded. Unwielding it to wield it
        // would be a loop, and it is the only loop guard the function has. Note the client
        // returns false directly and does **not** reset the unblock state,
        // so `blocking_id` survives with `unblock_attempt_num` at 0 -- unreadable until the next
        // walk zeroes it, which is exactly what happens.
        if blocking == blocked {
            return false;
        }
        let Some(player) = self.player else {
            self.unblock.reset();
            return false;
        };
        self.unblock.unblock_attempt_num += 1;
        self.unblock.blocked_id = Some(blocked);
        self.unblock.blocked_side = side;

        let name = self.weenie(blocking).map_or_else(String::new, |w| {
            w.display_name(
                crate::weenie::NameType::Appropriate,
                self.material_name(w.pwd.material_type.unwrap_or(0)),
            )
        });
        let mut text = format!("Moving {name} to your backpack");

        // Attempt to place the blocking item in the player's container: no requested
        // container, auto-merge on, place 0 — so `spill_target` picks the main pack and falls
        // through to the side packs when it is full. The split is the caller's, for the same
        // reason `auto_wear`'s is: the client reads out of a global.
        let ok = self.attempt_to_place_in_container(
            req,
            out,
            blocking,
            player,
            ObjectId(0),
            true,
            0,
            split,
            now,
        );
        if ok {
            self.unblock.blocking_dest_id = Some(player);
        } else {
            // The suffix is appended to the *same* line, and only when the blocker is
            // still a known object. One notice either way — the send is at the merge point.
            if self.weenie(blocking).is_some() {
                text.push_str(&format!(" - cannot unwield the {name}"));
            }
            self.unblock.reset();
        }
        out.emit(crate::Notice::DisplayString {
            channel: super::FEEDBACK_CHANNEL,
            text,
        });
        ok
    }

    /// The client's tail — **the
    /// retry**.
    ///
    /// The listener runs on every server move-item notice and fires only when all four of the
    /// client's own conditions hold. When an unblock is in flight (`unblock_attempt_num > 0`), the
    /// object that moved is the blocker, and a blocked item is recorded, it reads the blocked side
    /// and item and resets the unblock state — *before* the retry. Only if the fourth also holds,
    /// the blocker landed where it was sent (`blocking_dest_id == container`), does it retry
    /// `auto_wield(item, side, 0, 1, 0, 1)`, and when that answers `false` it clears the item's
    /// waiting state.
    ///
    /// **`unblock_attempt_num` never exceeds 1**, and that is not an accident of this rebuild: it is
    /// zeroed on entry to every wield that passes the legality check and
    /// incremented once, and retail never tests it against a
    /// bound. It is a *flag* — "an unblock is in flight" — not a counter, and both readers
    /// (the item-moved hook and the wield walk) test it as `> 0`. What actually bounds the recursion is
    /// the self-blocker guard: the retry re-enters the wield with `unblock = 1`, so a second blocker starts
    /// a second unblock, but a blocker that is the item itself stops immediately, and every
    /// successful round trip frees one slot.
    ///
    /// Note `other_side = 1` here where the paper doll's drag-accept passed `0`: after the round
    /// trip the retry *may* take the other wrist or ring.
    ///
    /// Returns `true` when the retry put the wield on the wire.
    #[allow(clippy::too_many_arguments)]
    pub fn unblock_on_item_moved(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        moved: ObjectId,
        container: ObjectId,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        if self.unblock.unblock_attempt_num == 0
            || self.unblock.blocking_id != Some(moved)
            || self.unblock.blocked_id.is_none()
        {
            return false;
        }
        let item = self.unblock.blocked_id.expect("just checked");
        let side = self.unblock.blocked_side;
        let dest = self.unblock.blocking_dest_id;
        self.unblock.reset();
        if dest != Some(container) {
            return false;
        }
        if self.auto_wield(req, out, item, side, false, true, true, split, now) {
            return true;
        }
        // The retry found nowhere to go, so the icon the drag ghosted comes back.
        self.set_waiting_state(item, false);
        false
    }

    /// Behavior: the server refused the
    /// blocker's move, so there is nothing to come back for.
    ///
    /// ```text
    /// if (unblock_attempt_num > 0 && blocking_id == failed) {
    ///     weenie(blocked_id).set_waiting_state(0);
    ///     blocking_id = blocked_id = blocked_side = unblock_attempt_num = 0;
    /// }
    /// ```
    ///
    /// The un-ghost is on `blocked_id` — the item the player dropped — and not on the blocker,
    /// whose own ghost has already cleared.
    ///
    /// **`failed` is the SUBSTITUTED object, not the one on the wire.** This is a
    /// notice handler: its only producer is the attempt-failed path, which raises it with
    /// **that function's own object** — and that object is whatever
    /// the network blob handler's `0x00A0` arm resolved after replacing the message's id with
    /// the previous request's object id. The retry hook has **no direct caller** in
    /// retail; it is reached only through the notice table. So a
    /// caller that passes the wire id compares the blocker against the wrong thing.
    pub fn unblock_on_attempt_failed(&mut self, failed: ObjectId) -> bool {
        if self.unblock.unblock_attempt_num == 0 || self.unblock.blocking_id != Some(failed) {
            return false;
        }
        if let Some(item) = self.unblock.blocked_id {
            self.set_waiting_state(item, false);
        }
        self.unblock.reset();
        true
    }

    /// The holder's split size equals its maximum for the item's whole stack — what the globals
    /// hold when no stack slider has been touched.
    #[must_use]
    pub fn whole_stack_of(&self, id: ObjectId) -> SplitState {
        let n = self.weenie(id).and_then(|w| w.pwd.stack_size).unwrap_or(0);
        SplitState::whole_stack(u32::from(n))
    }
}

/// Whether `plan` took one of `auto_wield`'s returns before the ready group.
fn wield_returns_before_ready(plan: &WieldPlan) -> bool {
    let WieldPlan::Wield(bit) = plan else {
        return false;
    };
    WIELD_SLOT_ORDER
        .iter()
        .any(|(candidate, _)| candidate == bit)
        || bit & (loc::WRIST_WEAR | loc::FINGER_WEAR) != 0
}

/// The paired-slot resolver — `side` names one of the pair's two slot records and the answer is
/// **that slot's own location**.
///
/// The null side falls through to the right-hand slot — the
/// side `auto_sort` passes.
#[must_use]
fn paired_side_slot(pair: u32, side: SlotSide) -> u32 {
    let (left, right) = if pair == loc::WRIST_WEAR {
        (loc::WRIST_WEAR_LEFT, loc::WRIST_WEAR_RIGHT)
    } else {
        (loc::FINGER_WEAR_LEFT, loc::FINGER_WEAR_RIGHT)
    };
    match side {
        SlotSide::Left => left,
        SlotSide::Right | SlotSide::Null => right,
    }
}

/// The null side has no opposite; `paired_side_slot` maps it to the right-hand slot, so the
/// "other side" is the left.
#[must_use]
fn opposite_side(side: SlotSide) -> SlotSide {
    match side {
        SlotSide::Left => SlotSide::Right,
        SlotSide::Right | SlotSide::Null => SlotSide::Left,
    }
}

/// The walk's tail: a recorded blocker becomes `Blocked`, nothing recorded means the item
/// named no slot this function knows.
/// `auto_wield`'s two walk-scoped fields: the blocking id (as the *slot* it
/// was found in, so [`finish_blocked`] can name the item) and the strings printed as the walk
/// passes each occupied slot.
///
/// A struct rather than a closure for one reason: the ready
/// group reads `blocking_id` back and tests it, and a closure
/// that holds `last_blocked` mutably cannot be read from while it is alive.
#[derive(Default)]
struct WieldWalk {
    last_blocked: Option<u32>,
    messages: Vec<&'static str>,
}

impl WieldWalk {
    /// The blocking-id store for the slot's item, and the check that guards the string.
    /// `None` is a slot retail blocks on **silently** — every arm of the ready group.
    fn block(&mut self, slot: u32, message: Option<&'static str>) {
        self.last_blocked = Some(slot);
        if let Some(m) = message {
            self.messages.push(m);
        }
    }
}

fn finish_blocked(w: &World, last_blocked: Option<u32>, messages: Vec<&'static str>) -> WieldPlan {
    match last_blocked {
        Some(slot) => WieldPlan::Blocked {
            slot,
            blocker: w.inv_slots.item_at(slot),
            messages,
        },
        None => WieldPlan::NotWieldable,
    }
}

/// What `auto_sort` decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoSortPlan {
    Wear(u32),
    Wield(u32),
    PlaceInBackpack,
    Refused,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weenie::Weenie;
    use dereth_protocol::types::PublicWeenieDesc;

    fn world_with(items: &[(u32, PublicWeenieDesc)]) -> World {
        let mut w = World::new();
        w.player = Some(ObjectId(1));
        for (id, pwd) in items {
            let mut wn = Weenie::new(ObjectId(*id));
            wn.pwd = pwd.clone();
            w.tables.weenies.insert(ObjectId(*id), wn);
        }
        w.tables.inventories.insert(
            ObjectId(1),
            crate::objects::ObjectInventory::new(ObjectId(1)),
        );
        w
    }

    fn wearable(valid: u32, priority: u32) -> PublicWeenieDesc {
        PublicWeenieDesc {
            name: "Helm".into(),
            valid_locations: Some(valid),
            priority: Some(priority),
            ..PublicWeenieDesc::default()
        }
    }

    /// Oracle: §9's, transcribed line by line.
    #[test]
    fn auto_wear_refuses_on_a_clothing_priority_clash_and_names_the_blocker() {
        let mut w = world_with(&[
            (10, wearable(loc::HEAD_WEAR, 0x4)),
            (11, wearable(loc::HEAD_WEAR, 0x4)),
        ]);
        assert_eq!(w.auto_wear_is_legal(ObjectId(10)), Ok(()));

        // Something already occupies priority 0x4.
        w.clothing_priority_mask = 0x4;
        w.inventory_mut(ObjectId(1))
            .unwrap()
            .set_placement(ObjectId(11), loc::HEAD_WEAR, 0x4);
        w.inventory_mask = loc::HEAD_WEAR;
        let (msg, worn) = w.auto_wear_is_legal(ObjectId(10)).unwrap_err();
        assert_eq!(msg, "You must remove your Helm to wear that");
        assert!(!worn);

        // The blocker is *this* item, already worn: with the item's own name.
        w.inventory_mut(ObjectId(1))
            .unwrap()
            .set_placement(ObjectId(10), loc::HEAD_WEAR, 0x4);
        let (msg, worn) = w.auto_wear_is_legal(ObjectId(10)).unwrap_err();
        assert_eq!(msg, "The Helm is already being worn");
        assert!(worn);

        // a blocker the client has no object for is a silent false.
        w.inventory_mut(ObjectId(1))
            .unwrap()
            .set_placement(ObjectId(10), 0, 0x4);
        w.inventory_mut(ObjectId(1))
            .unwrap()
            .set_placement(ObjectId(11), 0, 0x4);
        w.inventory_mut(ObjectId(1))
            .unwrap()
            .set_placement(ObjectId(99), loc::HEAD_WEAR, 0x4);
        assert_eq!(
            w.auto_wear_is_legal(ObjectId(10)),
            Err((String::new(), false))
        );
    }

    /// Oracle: §9's automatic-wear legality check — an item with no wearable bit is refused before anything
    /// else is examined.
    #[test]
    fn a_non_wearable_is_refused_outright() {
        let w = world_with(&[(10, wearable(loc::MELEE_WEAPON, 0))]);
        assert!(w.auto_wear_is_legal(ObjectId(10)).is_err());
    }

    /// Oracle: §9's, all three special cases with their strings.
    #[test]
    fn auto_wield_legality_covers_ammo_shield_and_held_in_combat() {
        let mut w = world_with(&[
            (
                10,
                PublicWeenieDesc {
                    name: "Arrow".into(),
                    valid_locations: Some(loc::MISSILE_AMMO),
                    ammo_type: Some(2),
                    ..PublicWeenieDesc::default()
                },
            ),
            (
                20,
                PublicWeenieDesc {
                    name: "Bow".into(),
                    ammo_type: Some(1),
                    location: Some(loc::MISSILE_WEAPON),
                    ..PublicWeenieDesc::default()
                },
            ),
            (
                30,
                PublicWeenieDesc {
                    name: "Shield".into(),
                    valid_locations: Some(loc::SHIELD),
                    ..PublicWeenieDesc::default()
                },
            ),
            (
                40,
                PublicWeenieDesc {
                    name: "Torch".into(),
                    valid_locations: Some(loc::HELD),
                    ..PublicWeenieDesc::default()
                },
            ),
            (
                50,
                PublicWeenieDesc {
                    name: "Wand".into(),
                    valid_locations: Some(loc::HELD),
                    obj_type: item_type::CASTER,
                    ..PublicWeenieDesc::default()
                },
            ),
        ]);
        w.inv_slots.set(loc::WEAPON_READY_SLOT, ObjectId(20));
        assert_eq!(
            w.auto_wield_is_legal(ObjectId(10)),
            Err("Cannot be used with Bow".into())
        );

        // A bow with an ammo type and `combat_use == MISSILE` blocks
        // the shield; so does `combat_use == TWO_HANDED` and so does a caster. `_location` is not
        // an input — the earlier transcription tested `location & TWO_HANDED`, which is nothing
        // the function reads., and `NAME_SINGULAR` for the blocker.
        w.weenie_mut(ObjectId(20)).unwrap().pwd.combat_use =
            Some(crate::combat::combat_use::MISSILE);
        assert_eq!(
            w.auto_wield_is_legal(ObjectId(30)),
            Err("A shield may not be worn with the Bow".into())
        );
        w.weenie_mut(ObjectId(20)).unwrap().pwd.combat_use = Some(crate::combat::combat_use::MELEE);
        assert_eq!(
            w.auto_wield_is_legal(ObjectId(30)),
            Ok(()),
            "a one-handed melee weapon does not"
        );
        w.weenie_mut(ObjectId(20)).unwrap().pwd.combat_use =
            Some(crate::combat::combat_use::TWO_HANDED);
        assert_eq!(
            w.auto_wield_is_legal(ObjectId(30)),
            Err("A shield may not be worn with the Bow".into())
        );
        w.weenie_mut(ObjectId(20)).unwrap().pwd.location = Some(loc::TWO_HANDED);
        w.weenie_mut(ObjectId(20)).unwrap().pwd.combat_use = Some(crate::combat::combat_use::MELEE);
        assert_eq!(
            w.auto_wield_is_legal(ObjectId(30)),
            Ok(()),
            "the ready weapon's location is not an input of the shield-blocking test"
        );

        // A held item out of combat is fine; in combat only a caster is..
        assert_eq!(w.auto_wield_is_legal(ObjectId(40)), Ok(()));
        w.combat.combat_mode = crate::combat::CombatMode::Melee;
        assert_eq!(
            w.auto_wield_is_legal(ObjectId(40)),
            Err("Cannot hold Torch while in combat".into())
        );
        assert_eq!(
            w.auto_wield_is_legal(ObjectId(50)),
            Ok(()),
            "a caster is allowed"
        );
    }

    /// Blocks use of shield is combat use and type not location.
    #[test]
    fn blocks_use_of_shield_is_combat_use_and_type_not_location() {
        use crate::combat::combat_use;
        let mk = |cu: u8, ammo: u16, ty: u32| {
            let mut w = crate::Weenie::new(ObjectId(1));
            w.pwd.combat_use = Some(cu);
            w.pwd.ammo_type = Some(ammo);
            w.pwd.obj_type = ty;
            w
        };
        assert!(
            blocks_use_of_shield(&mk(combat_use::MISSILE, 1, 0)),
            "a missile weapon with ammunition blocks a shield"
        );
        assert!(
            !blocks_use_of_shield(&mk(combat_use::MISSILE, 0, 0)),
            "a missile weapon without ammunition does not block a shield"
        );
        assert!(
            blocks_use_of_shield(&mk(combat_use::TWO_HANDED, 0, 0)),
            "a two-handed weapon blocks a shield"
        );
        assert!(
            blocks_use_of_shield(&mk(combat_use::MELEE, 0, item_type::CASTER)),
            "a caster blocks a shield"
        );
        assert!(!blocks_use_of_shield(&mk(combat_use::MELEE, 0, 0)));
        assert!(!blocks_use_of_shield(&mk(combat_use::NONE, 5, 0)));
    }

    /// The wield slot order is the clients and not ascending.
    #[test]
    fn the_wield_slot_order_is_the_clients_and_not_ascending() {
        assert_eq!(
            WIELD_SLOT_ORDER[0].0,
            loc::NECK_WEAR,
            "the walk tests the neck bit first"
        );
        assert_eq!(WIELD_SLOT_ORDER[1].0, loc::TRINKET_ONE);
        assert_eq!(WIELD_SLOT_ORDER[2].0, loc::CLOAK);
        assert_eq!(WIELD_SLOT_ORDER[6].0, loc::HEAD_WEAR);
        assert!(
            WIELD_SLOT_ORDER[1].0 > WIELD_SLOT_ORDER[6].0,
            "the second tested bit is numerically larger than the seventh"
        );
        // And the five locations that are *not* in this walk, because they are not sent as the
        // table's own mask: (the paired slots) and (the ready group).
        for absent in [
            loc::WRIST_WEAR,
            loc::FINGER_WEAR,
            loc::WEAPON_READY_SLOT,
            loc::MISSILE_AMMO,
            loc::SHIELD,
        ] {
            assert!(
                !WIELD_SLOT_ORDER.iter().any(|(b, _)| *b == absent),
                "{absent:#010X} is handled by its own code, not by the single-bit walk"
            );
        }

        let mut w = world_with(&[(10, wearable(loc::TRINKET_ONE | loc::HEAD_WEAR, 0))]);
        assert_eq!(
            w.plan_auto_wield(ObjectId(10), SlotSide::Null, false),
            WieldPlan::Wield(loc::TRINKET_ONE)
        );
        // Occupy the trinket slot and the head is chosen next.
        w.inventory_mask = loc::TRINKET_ONE;
        assert_eq!(
            w.plan_auto_wield(ObjectId(10), SlotSide::Null, false),
            WieldPlan::Wield(loc::HEAD_WEAR)
        );
        w.inventory_mask = loc::TRINKET_ONE | loc::HEAD_WEAR;
        assert_eq!(
            w.plan_auto_wield(ObjectId(10), SlotSide::Null, false),
            WieldPlan::Blocked {
                slot: loc::HEAD_WEAR,
                blocker: None,
                messages: vec![
                    "You're already wearing a trinket.",
                    "You're already wearing a helm.",
                ],
            }
        );
    }

    /// The walk's equal-WCID ammunition refusal does not overwrite a blocker recorded earlier
    /// in the walk. This matters when the ready merge is full: the tail still acts on the earlier
    /// slot rather than moving the equipped ammunition stack.
    #[test]
    fn full_matching_ammunition_preserves_the_walks_earlier_blocker() {
        let mut w = world_with(&[
            (
                10,
                PublicWeenieDesc {
                    name: "Arrow".into(),
                    wcid: 7,
                    valid_locations: Some(loc::HEAD_WEAR | loc::MISSILE_AMMO),
                    stack_size: Some(7),
                    max_stack_size: Some(10),
                    ..PublicWeenieDesc::default()
                },
            ),
            (20, wearable(loc::HEAD_WEAR, 0)),
            (
                30,
                PublicWeenieDesc {
                    name: "Arrow".into(),
                    wcid: 7,
                    location: Some(loc::MISSILE_AMMO),
                    stack_size: Some(10),
                    max_stack_size: Some(10),
                    ..PublicWeenieDesc::default()
                },
            ),
        ]);
        w.inventory_mask = loc::HEAD_WEAR | loc::MISSILE_AMMO;
        w.inv_slots.set(loc::HEAD_WEAR, ObjectId(20));
        w.inv_slots.set(loc::MISSILE_AMMO, ObjectId(30));

        assert_eq!(
            w.plan_auto_wield(ObjectId(10), SlotSide::Right, false),
            WieldPlan::Blocked {
                slot: loc::HEAD_WEAR,
                blocker: Some(ObjectId(20)),
                messages: vec!["You're already wearing a helm."],
            }
        );
    }

    /// Oracle: §9 — `side` picks the slot when both sides of a paired location are valid.
    #[test]
    fn side_disambiguates_the_paired_wrist_and_finger_slots() {
        let mut w = world_with(&[(10, wearable(loc::FINGER_WEAR, 0))]);
        assert_eq!(
            w.plan_auto_wield(ObjectId(10), SlotSide::Left, false),
            WieldPlan::Wield(loc::FINGER_WEAR_LEFT)
        );
        assert_eq!(
            w.plan_auto_wield(ObjectId(10), SlotSide::Right, false),
            WieldPlan::Wield(loc::FINGER_WEAR_RIGHT)
        );

        // `other_side`: with the aimed-at side full, `auto_sort` and the unblock retry
        // take the other half of the pair and a paper-doll drop does not.
        w.inventory_mask = loc::FINGER_WEAR_LEFT;
        assert_eq!(
            w.plan_auto_wield(ObjectId(10), SlotSide::Left, true),
            WieldPlan::Wield(loc::FINGER_WEAR_RIGHT),
            "other_side = 1 opens the other ring"
        );
        assert_eq!(
            w.plan_auto_wield(ObjectId(10), SlotSide::Left, false),
            WieldPlan::Blocked {
                slot: loc::FINGER_WEAR_LEFT,
                blocker: None,
                messages: vec![FINGER_BLOCKED_MESSAGE],
            },
            "other_side = 0 stops at the slot the player aimed at"
        );
    }

    /// Oracle: §9's — the four-step order, including the `if (worn) return
    /// false` early exit.
    #[test]
    fn auto_sort_follows_wear_then_wield_then_backpack() {
        let mut w = world_with(&[
            (10, wearable(loc::HEAD_WEAR, 0x4)),
            (
                20,
                PublicWeenieDesc {
                    valid_locations: Some(loc::MELEE_WEAPON),
                    ..PublicWeenieDesc::default()
                },
            ),
            (30, PublicWeenieDesc::default()),
        ]);
        assert_eq!(
            w.plan_auto_sort(ObjectId(10), true),
            AutoSortPlan::Wear(loc::HEAD_WEAR)
        );
        assert_eq!(
            w.plan_auto_sort(ObjectId(20), true),
            AutoSortPlan::Wield(loc::MELEE_WEAPON)
        );
        assert_eq!(
            w.plan_auto_sort(ObjectId(20), false),
            AutoSortPlan::PlaceInBackpack,
            "with wielding disallowed an unowned item falls through to the backpack"
        );
        assert_eq!(
            w.plan_auto_sort(ObjectId(30), true),
            AutoSortPlan::PlaceInBackpack
        );

        // Already worn: the walk stops rather than falling through to wield.
        w.clothing_priority_mask = 0x4;
        w.inventory_mask = loc::HEAD_WEAR;
        w.inventory_mut(ObjectId(1))
            .unwrap()
            .set_placement(ObjectId(10), loc::HEAD_WEAR, 0x4);
        assert_eq!(w.plan_auto_sort(ObjectId(10), true), AutoSortPlan::Refused);
    }
}
