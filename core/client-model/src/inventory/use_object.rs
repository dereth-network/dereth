//! Behavior: what a use on a **loose** object does, and the pickup it
//! dispatches to.
//!
//! # Which message carries a pickup
//!
//! **There is no pickup message.** The client has no pickup event; `0xF74A
//! Inventory_PickupEvent` is the *server* telling the client an object left the 3-D world. The
//! client picks an item up by sending the `0x0019` put-in-container request `(item, playerId, 0)` —
//! exactly the message a drag into the pack sends — and the only thing that makes it a *pickup*
//! rather than a *move* is that the request tracker records a pickup when the
//! item is in the 3-D view. This module is everything that *reaches* that request from a
//! double-click.
//!
//! The chain, and it is entirely client-side up to the wire:
//!
//! ```text
//! viewport double-click -> pick object
//!   -> reject an object wielded by the player
//!   -> use_object -> determine_use_result
//!        if 1 < result < 8: dispatch the item-use action and return
//!   -> result 2: place in backpack
//!   -> place in container -> send 0x0019 and record the pickup request
//! ```
//!
//! The `return` on the fast path is the whole answer. A use on a **wielded**
//! or **contained** object falls out of `determine_use_result`'s first gate into the general path and
//! ends at the use request (`0x0036`). A use on a **loose** one never reaches that
//! line: it returns 2 and sends `0x0019` instead. Sending `0x0036` for both would leave nothing
//! acquirable.
//!
//! # The rest of it: the arms that open a panel, and the ground container
//!
//! **The item-use action runs on four paths, not only the fast path (`1 < r < 8`).**
//! The other three are the general path: after the use request, after the callback a
//! confirmation dialog runs, and — with the consume-selection flag 0 — on the *not useable at
//! all* arm. `using_item`'s **tail** runs whatever its switch did:
//!
//! ```text
//! if owned_by_player && container: open the contained-container panel
//! if usable && !targeted && container && !owned_by_player: attempt to open as ground object
//! ```
//!
//! A corpse is a container, is useable, and is not yours, so the second line is the one that
//! makes it the open ground object — and `determine_use_result`'s first gate exempts anything
//! contained in *that*, which is what lets loot inside it be double-clicked into the pack. That
//! line is the ground-object field's **only production writer**: without it a running client
//! believes nothing is ever an open ground container, so **looting a corpse is unreachable even
//! though the pickup machinery under it works**. The first line is the same shape one level
//! down: without it a double-click on one of your own packs opens nothing.
//!
//! Also here: arms 5, 6 and 7 (`attempt_to_open_trade_negotiations`, the salvage panel and
//! `BeginGame`); the three usage confirmations, without which a PK altar would be used without
//! asking; the general-path refusals and their messages; and the ground-object setter that
//! raises the panel.
//!
//! # What this module still does not do
//!
//! The component-pack classification table is not loaded from the local DAT, so a component
//! pouch on the ground is opened rather than picked up.

use crate::inventory::requests::messages as request_messages;
use crate::inventory::SplitState;
use crate::world::World;
use crate::{Notice, NoticeSink, RequestSink};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::types::ContentProfile;
use {crate::weenie::item_useable, crate::weenie::Weenie, dereth_rules::weenie::item_type};

/// Object-description bits used by result classification and item use.
///
/// These are bits the curated list in [`crate::weenie::bitfield`] deliberately leaves unnamed
/// because the general object accessors do not read them. They are named here at their use
/// sites. The names are ACE's `ObjectDescriptionFlag`
/// enumeration.
pub mod use_bitfield {
    /// Bit 23. Result classification groups it with a non-zero item capacity or
    /// container capacity: an object that needs a pack slot is not something a double-click
    /// stuffs into the backpack.
    pub const REQUIRES_PACK_SLOT: u32 = 0x0080_0000;
    /// Bit 10. Ask for PK-altar confirmation and return without sending. ACE calls it `PkSwitch`.
    pub const PK_ALTAR: u32 = 0x0000_0400;
    /// Bit 11. Ask for non-PK-altar confirmation. ACE calls it `NpkSwitch`.
    pub const NPK_ALTAR: u32 = 0x0000_0800;
    /// Bit 12. The one bitfield test in the use path's **not-useable** arm: it selects a different
    /// refusal message from the string table. ACE calls it `Door`, and a locked door is exactly
    /// the object that reaches that arm.
    pub const DOOR: u32 = 0x0000_1000;
    /// Bit 28. Consult the character's rare-use confirmation option first.
    pub const VOLATILE_RARE: u32 = 0x1000_0000;
    /// Bit 29. Using the item wields it.
    pub const WIELD_ON_USE: u32 = 0x2000_0000;
    /// Bit 30. Result classification's wield arm yields 3 when this bit is clear and 8 when it
    /// is set, the right hand versus the left: the use handler wields arm 3 on the right side and
    /// arm 8 on the left.
    pub const WIELD_LEFT: u32 = 0x4000_0000;
}

/// The valid-locations groups tested for a free slot, in the client's order.
///
/// Each is "the item can go somewhere in this group and is currently in none of it".
const AUTO_SORT_GROUPS: [u32; 3] = [
    dereth_rules::slots::loc::ARMOR,
    dereth_rules::slots::loc::CLOTHING,
    // Neck, both wrists, both fingers, the trinket and the three sigils.
    0x7C0F_8000,
];

/// The item-usability predicates: three over the source (low) half of an object's useability
/// word, which the use path reads, and two over the target (high) half, which the cursor and the
/// HUD read.
///
/// The usable predicate tests the complement of bit 0, so an
/// object with **no** useability at all (`USEABLE_UNDEF`, 0) is useable. Only the explicit
/// `USEABLE_NO` bit makes it not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemUses(pub u32);

impl ItemUses {
    /// An object is usable unless bit 0 explicitly forbids it.
    #[must_use]
    pub fn is_useable(self) -> bool {
        self.0 & 1 == 0
    }

    /// Behavior: the high half, shifted down sixteen.
    #[must_use]
    pub fn is_useable_targeted(self) -> bool {
        self.0 & item_useable::TARGET_MASK != 0
    }

    /// Behavior: remote, then viewed, then contained, then
    /// wielded, then whatever is left of the low byte masked to `USEABLE_SELF`.
    #[must_use]
    pub fn least_limited_source_use(self) -> u32 {
        for bit in [0x20u32, 0x10, 0x08, 0x04] {
            if self.0 & bit != 0 {
                return bit;
            }
        }
        self.0 & 0xFF & 0x02
    }

    /// Tests bit 1 after shifting the high half down sixteen bits.
    ///
    /// The client writes the shift as a 16-iteration loop; it is `(bits >> 16) & USEABLE_SELF`.
    #[must_use]
    pub fn is_useable_self_target(self) -> bool {
        (self.0 & item_useable::TARGET_MASK) >> 16 & item_useable::SELF != 0
    }

    /// Chooses the least restricted target use after shifting the high half down sixteen, trying
    /// remote, viewed, contained, wielded, self, and finally `& USEABLE_OBJSELF`.
    ///
    /// Note the tail differs from the least-limited source use, which stops at the low byte's
    /// `USEABLE_SELF`: the target form has the extra `USEABLE_SELF` test **and** the
    /// `USEABLE_OBJSELF` fallback. Both are reproduced, because the target-compatibility test
    /// checks the result for `USEABLE_CONTAINED` and `USEABLE_WIELDED` and a wrong fallback would
    /// change which of the two arms an item takes.
    #[must_use]
    pub fn least_limited_target_use(self) -> u32 {
        let t = (self.0 & item_useable::TARGET_MASK) >> 16;
        for bit in [
            item_useable::REMOTE,
            item_useable::VIEWED,
            item_useable::CONTAINED,
            item_useable::WIELDED,
            item_useable::SELF,
        ] {
            if t & bit != 0 {
                return bit;
            }
        }
        t & item_useable::OBJSELF
    }
}

/// Result classification used by the item-use dispatcher.
///
/// The use path's fast path is `1 < r && r < 8`, so [`UseResult::Nothing`], [`UseResult::Useable`]
/// **and** [`UseResult::WieldLeft`] fall through to the general path. That 8 is excluded from a
/// range whose switch handles it is the client's, not a transcription slip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum UseResult {
    /// 0 — nothing special; the general path decides.
    Nothing = 0,
    /// 1 — useable; the general path sends the use request.
    Useable = 1,
    /// 2 — **the pickup**: `place_in_backpack`.
    PlaceInBackpack = 2,
    /// 3 — `auto_wield(SlotSide::Right)`.
    WieldRight = 3,
    /// 4 — `auto_sort`: wear it, else wield it.
    AutoSort = 4,
    /// 5 — `attempt_to_open_trade_negotiations`.
    Trade = 5,
    /// 6 — the open-salvage-panel notice.
    Salvage = 6,
    /// 7 — Start interaction with a game board.
    BeginGame = 7,
    /// 8 — `auto_wield(SlotSide::Left)`.
    WieldLeft = 8,
}

impl UseResult {
    /// The use path's test: not a second click, and `1 < r && r < 8`.
    #[must_use]
    pub fn takes_the_fast_path(self) -> bool {
        let n = self as u16;
        1 < n && n < 8
    }
}

/// The three confirmation dialogs raised before sending an item-use request.
///
/// Each creates a callback dialog in the current UI, and the `Yes` half of every one of them is
/// one function, because the client's confirmation callback is one function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageConfirmation {
    /// Behavior: bitfield bit [`use_bitfield::PK_ALTAR`].
    PkAltar,
    /// Behavior: bit [`use_bitfield::NPK_ALTAR`].
    NpkAltar,
    /// Behavior: bit [`use_bitfield::VOLATILE_RARE`], and only
    /// when the character option says to ask.
    VolatileRare,
}

impl UsageConfirmation {
    /// The dialog's text.
    ///
    /// Each confirmation has a fixed prompt, taken from retail.
    #[must_use]
    pub fn prompt(self) -> &'static str {
        match self {
            Self::PkAltar => {
                "Using this altar will make you a player killer, able to attack or be attacked \
                 by other player killers. Are you sure you want to do this?"
            }
            Self::NpkAltar => {
                "Using this altar will make you a non-player killer, unable to attack or be \
                 attacked by other player killers. Are you sure you want to do this?"
            }
            Self::VolatileRare => "Are you sure you want to use this rare item?",
        }
    }
}

/// The two lines a *use* prints on channel `0x1A`, and the use path's other literals.
///
/// The creature-type bit `0x10`
/// selects *"Approaching"*; other objects, such as chests, print *"Using the"*.
///
/// **Confirmation callbacks have no such branch**: they use
/// `using_the`'s literal unconditionally. That asymmetry is the client's and
/// is kept in the usage-confirmation callback.
pub mod messages {
    /// `"Using the %s"`. `%s` is the object's name of type 2, i.e.
    /// [`crate::weenie::NameType::Appropriate`].
    #[must_use]
    pub fn using_the(name: &str) -> String {
        format!("Using the {name}")
    }

    /// `"The %s is locked"`, used by the container-opening arm and the drop/place checks.
    ///
    /// This is the whole of *"The Aluvian Pathwarden Chest is locked"*: ACE sends
    /// **no** text for a locked-chest use unless `fix_chest_missing_inventory_window` is on
    /// (measured live: `0xF750 OpenFailDueToLock` + `0x01C7 UseDone(0)`
    /// and nothing else), and retail prints this line itself, **before** *"Using the %s"* --
    /// the use path dispatches the container action before composing its own final message.
    #[must_use]
    pub fn is_locked(name: &str) -> String {
        format!("The {name} is locked")
    }

    /// `"Approaching %s"` -- the `ITEM_TYPE_CREATURE` arm. **No definite article**:
    /// the literal is `"Approaching %s"`, not `"Approaching the %s"`.
    #[must_use]
    pub fn approaching(name: &str) -> String {
        format!("Approaching {name}")
    }

    /// `"You cannot use the %s because you are trading it"`, pushed at
    /// the `trade_state == 1` arm.
    #[must_use]
    pub fn cannot_use_because_trading(name: &str) -> String {
        format!("You cannot use the {name} because you are trading it")
    }

    /// `"You must wield the %s to use it"`, pushed — the arm where the item is not wielded
    /// and its least-limited source use includes `USEABLE_WIELDED`.
    #[must_use]
    pub fn must_wield_to_use(name: &str) -> String {
        format!("You must wield the {name} to use it")
    }

    /// `"Choose a target for the %s"`, pushed — the
    /// targeted-use arm that sets the use-target target mode.
    ///
    /// Printed *after* the targeting object is recorded and target mode 3 is armed, so arming
    /// the mode and saying so are one act: this is not a refusal but the prompt for the second
    /// click. It is in this module because it is one of the use path's seven literals and shares
    /// their format-and-send tail.
    #[must_use]
    pub fn choose_a_target_for(name: &str) -> String {
        format!("Choose a target for the {name}")
    }

    /// `"You can't open or close this %s that way"`, pushed —
    /// the not-useable [`super::use_bitfield::DOOR`] arm.
    #[must_use]
    pub fn cannot_open_or_close_that_way(name: &str) -> String {
        format!("You can't open or close this {name} that way")
    }

    /// `"To attack %s, click on the dove icon first"`, pushed —
    /// the not-useable, **attackable**, combat-mode-`NONCOMBAT` arm.
    ///
    /// **No definite article**, like [`approaching`]: the literal is `"To attack %s"`.
    #[must_use]
    pub fn to_attack_click_the_dove(name: &str) -> String {
        format!("To attack {name}, click on the dove icon first")
    }

    /// `"The %s cannot be used"`, pushed — the use path's last
    /// line, and the fall-through for everything not useable that is neither a door nor
    /// attackable.
    #[must_use]
    pub fn cannot_be_used(name: &str) -> String {
        format!("The {name} cannot be used")
    }

    /// `"Select your target before using the %s"`.
    /// **Taken from retail and deliberately left without a producer.**
    ///
    /// It is the targeted-use arm's other leg: consume-selection set and no selected object.
    /// The `use_object` method does not carry the consume-selection flag —
    /// see its own note on the three producers and why the value is inert for every one of them
    /// — so this leg is unreachable here, and a `fn` for it would be one more caller-less item.
    /// The wording is recorded as a constant so that whoever adds the flag has it.
    pub const SELECT_YOUR_TARGET_BEFORE_USING: &str = "Select your target before using the %s";
}

/// Why the client's general path sent nothing.
///
/// 1. **They are not string-table rows.** Every one is a wide literal the client passes
///    straight to its wide-string formatting call. All seven are in [`messages`].
/// 2. **Two of these are silent.** [`Self::ItIsYou`] is the player-self check:
///    a bare `return` with no literal, exactly like [`Self::AttackingIt`].
/// 3. **Retail computes seven reasons.** An unusable, attackable object with the combat mode
///    `NONCOMBAT` prints *"To attack %s, click on the dove icon first"*, not
///    [`Self::NotUseable`]'s *"The %s cannot be used"* — and a monster clicked out of combat is
///    the commonest way a player meets this function at all.
///
/// [`Self::text`] is the mapping, and the only place the choice is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseRefusal {
    /// `trade_state == 1` — the item is on the trade window.
    /// [`messages::cannot_use_because_trading`].
    BeingTraded,
    /// Not wielded, and the least-limited source use includes `USEABLE_WIELDED` — "you must be
    /// wielding it". [`messages::must_wield_to_use`].
    MustBeWielded,
    /// Not useable at all, and the object is the player himself: retail `return`s with no message.
    ItIsYou,
    /// Not useable, and [`use_bitfield::DOOR`] is set.
    /// [`messages::cannot_open_or_close_that_way`].
    Door,
    /// Not useable, attackable, and the player is **in** a combat stance: retail
    /// `return`s silently, because the click was an attack rather than a use.
    AttackingIt,
    /// Not useable, attackable, and the player is in
    /// [`crate::combat::CombatMode::NonCombat`], whose raw value is 1.
    /// [`messages::to_attack_click_the_dove`].
    ///
    /// The two attackability checks repeat one predicate: take this arm on attackable-and-
    /// NonCombat, returns silently on attackable-and-in-a-stance
    /// ([`Self::AttackingIt`]), and not-attackable reaches [`Self::NotUseable`] from either.
    AttackDoveIcon,
    /// Not useable, and none of the four above. [`messages::cannot_be_used`].
    NotUseable,
}

impl UseRefusal {
    /// The line printed for this reason, with `%s` filled in
    /// from the object's name of type 2 — or `None` for the two arms that `return`
    /// with no message at all.
    ///
    /// `None` is not "the text is unknown": it is retail's behaviour, and the two are different
    /// facts for the same reason this is an enum and not a `bool`.
    #[must_use]
    pub fn text(self, name: &str) -> Option<String> {
        match self {
            Self::BeingTraded => Some(messages::cannot_use_because_trading(name)),
            Self::MustBeWielded => Some(messages::must_wield_to_use(name)),
            Self::Door => Some(messages::cannot_open_or_close_that_way(name)),
            Self::AttackDoveIcon => Some(messages::to_attack_click_the_dove(name)),
            Self::NotUseable => Some(messages::cannot_be_used(name)),
            Self::ItIsYou | Self::AttackingIt => None,
        }
    }
}

/// The outcome of an item-use attempt, so callers can count and test it.
///
/// **A note on the tail.** None of these variants carries what
/// the item-use tail did, because its effects are notices and world
/// state, such as [`Notice::OpenContainedContainer`]. A caller that wants
/// them should read those rather than a summary. Asserting the notice is asserting the thing the
/// panel will act on; asserting a `bool` here is asserting this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseOutcome {
    /// No weenie for the id.
    NoObject,
    /// The 200 ms interval since the previous use has not elapsed.
    Throttled,
    /// The inventory-request readiness check refused: the lock, or an attack.
    Busy,
    /// The fast path ran. Carries the result and whether this module dispatched it.
    Dispatched { result: UseResult, sent: bool },
    /// The general path sent (0x0036).
    UseEventSent,
    /// Targeted use — the item becomes the targeting object and target mode
    /// `TargetMode::UseTarget` is armed; the second click sends `0x0035`. The target mode
    /// itself is `dereth-client`'s `Interaction`, not this crate's, which is why this is a distinct
    /// outcome rather than [`Self::Nothing`].
    TargetModeArmed,
    /// A confirmation dialog stands in the way and **nothing was sent**;
    /// [`Notice::UsageConfirmation`] carries it; accepting calls the usage-confirmation handler.
    Confirming(UsageConfirmation),
    /// The general path refused, for the client's own reason.
    Refused(UseRefusal),
    /// The general path found nothing to send.
    Nothing,
}

/// The outcome of attempting to open a ground object.
///
/// Distinct outcomes rather than a `bool`, because "the object refuses to open" and "the client never
/// asked" are different facts and the second is the one a silent instrument hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroundObjectResult {
    /// The setter ran and this is now the ground object.
    Opened,
    /// It already was; the ground-object setter's "is this a different object" guard did nothing.
    AlreadyOpen,
    /// The non-quiet inventory-request readiness check refused — the lock, or an attack. The client's
    /// own message was shown.
    Busy,
    /// No weenie for the id. The message is a retail literal and is shown.
    NoRepresentation,
    /// `(bitfield & OPENABLE) == 0`. A **creature** is refused silently; anything else gets
    /// [`messages::is_locked`] -- *"The %s is locked"*, retail's own literal.
    NotOpenable { creature: bool },
}

/// Minimum interval between item-use attempts, in seconds.
pub const USE_THROTTLE: f64 = 0.2;

impl World {
    /// Returns whether this weenie class id is in the component-pack table.
    ///
    /// This requires a component-pack classification lookup that the crate does not perform,
    /// so this is **always false** and a spell-component pouch lying on the ground takes the
    /// container path (opened) instead of the pickup path. The table is not
    /// loaded.
    #[must_use]
    pub fn is_component_pack(&self, _item: ObjectId) -> bool {
        false
    }

    /// Classify an object's use action.
    ///
    /// Three gates decide whether an object is "loose enough to pick up", and then a general block
    /// classifies everything that is not.
    ///
    /// 1. **Where it is.** Contained, or `STUCK` (bit 2, "cannot be picked up"), sends it to the
    ///    general block — *unless* its container is the currently open ground object, which is what
    ///    makes double-clicking loot inside a corpse take it.
    /// 2. **Who holds it.** Wielded by somebody else: general block.
    /// 3. **What it is.** Needs a pack slot, or has an item or container capacity of its own, and
    ///    is not a component pack: general block. This is what stops a chest, a corpse or another
    ///    player from being stuffed into the backpack.
    ///
    /// Anything that passes all three is [`UseResult::PlaceInBackpack`]. Note that a *creature*
    /// passes all three — it is refused one layer later by
    /// the item-legality check's "You cannot pick up creatures!", which is why
    /// that string exists.
    #[must_use]
    pub fn determine_use_result(&self, item: ObjectId) -> UseResult {
        let Some(w) = self.weenie(item) else {
            return UseResult::Nothing;
        };
        let container = w.pwd.container_id.unwrap_or_default();
        let stuck = w.pwd.bitfield & dereth_rules::weenie::bitfield::STUCK != 0;
        let mut general = false;
        if container.0 != 0 || stuck {
            general = self.ground_object.is_none_or(|g| container != g);
        }
        if !general {
            if let Some(wielder) = w.pwd.wielder_id.filter(|x| x.0 != 0) {
                general = Some(wielder) != self.player;
            }
        }
        if !general {
            let needs_a_slot = w.pwd.bitfield & use_bitfield::REQUIRES_PACK_SLOT != 0
                || w.pwd.items_capacity.unwrap_or(0) != 0
                || w.pwd.containers_capacity.unwrap_or(0) != 0;
            general = needs_a_slot && !self.is_component_pack(item);
        }
        if !general {
            return UseResult::PlaceInBackpack;
        }
        self.general_use_result(w, item)
    }

    /// The general-use fallback handles everything the three earlier gates rejected.
    fn general_use_result(&self, w: &Weenie, item: ObjectId) -> UseResult {
        let obj_type = w.inq_type();
        if self.is_owned_by_player(item) {
            // Combat-usable, a caster, or "wield on use", and not already wielded by me.
            let wieldable = w.pwd.combat_use.unwrap_or(0) != 0
                || obj_type & item_type::CASTER != 0
                || w.pwd.bitfield & use_bitfield::WIELD_ON_USE != 0;
            if wieldable && w.pwd.wielder_id.unwrap_or_default() != self.player.unwrap_or_default()
            {
                return if w.pwd.bitfield & use_bitfield::WIELD_LEFT != 0 {
                    UseResult::WieldLeft
                } else {
                    UseResult::WieldRight
                };
            }
            let valid = w.pwd.valid_locations.unwrap_or(0);
            let location = w.pwd.location.unwrap_or(0);
            if AUTO_SORT_GROUPS
                .iter()
                .any(|g| valid & g != 0 && location & g == 0)
            {
                return UseResult::AutoSort;
            }
            if obj_type & item_type::TINKERING_TOOL != 0 {
                return UseResult::Salvage;
            }
        } else if (obj_type as i32) < 0 {
            // `TYPE_GAMEBOARD`, tested as the sign bit of the item type.
            return UseResult::BeginGame;
        }
        if ItemUses(w.pwd.useability.unwrap_or(0)).is_useable() {
            return UseResult::Useable;
        }
        if w.is_player() && Some(item) != self.player {
            return UseResult::Trade;
        }
        UseResult::Nothing
    }

    /// Use the selected object: double-click, the toolbar's Use button
    /// and the `USE` action all arrive here.
    ///
    /// The two arguments the retail signature carries and this one does not are fixed at every
    /// call site this client has: the consume-selection flag ("a targeted use may consume
    /// the selection") and the second-click flag ("this *is* the second click of a targeted use",
    /// 1 only from the spell-casting panel) are both 0 for the three gestures wired here.
    ///
    /// **The consume-selection flag has three producers**: the plugin API, the spell-casting
    /// panel, and the client's `case 0x13` and `case 0x14` — the
    /// `SelectionUseClosestUnopenedCorpse` / `SelectionUseNextUnopenedCorpse` tails, which use
    /// the selected object with the flag **1** and the second-click flag 0, and which are wired.
    /// **The value is still inert for what those two arms can hand it**, because
    /// the use path reads the flag only inside its targeted-use arm and a corpse
    /// is useable, not targeted-useable — it takes the use-request branch, where the
    /// two values agree.
    ///
    /// A rebuild that needs either parameter should add it then rather than carry two dead ones
    /// now — but it should add the consume-selection flag knowing there are **three** producers.
    ///
    /// `now` is for the 200 ms time-last-used throttle, which is what stops a
    /// double-click's two deliveries from firing two pickups.
    pub fn use_object(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        split: SplitState,
        now: ServerTime,
    ) -> UseOutcome {
        if self.weenie(item).is_none() {
            return UseOutcome::NoObject;
        }
        // Return if now is before last_use + 0.2. Use `None` rather than 0.0 because the
        // original timer and static both start at zero and the resulting 200 ms dead zone
        // at process start is not a behaviour anything can observe.
        if self.last_used.is_some_and(|t| now.0 < t.0 + USE_THROTTLE) {
            return UseOutcome::Throttled;
        }
        self.last_used = Some(now);
        if let Err(e) = self.ready_for_inventory_request() {
            self.refuse(out, false, e);
            return UseOutcome::Busy;
        }
        // An item whose container is the open vendor returns at once — a vendor's stock is
        // bought, not used. This follows readiness and is deliberately a
        // silent return before result classification can reach the use request.
        if self.shop.vendor_id.is_some_and(|vendor| {
            vendor.0 != 0
                && self.weenie(item).and_then(|item| item.pwd.container_id) == Some(vendor)
        }) {
            return UseOutcome::Nothing;
        }
        let result = self.determine_use_result(item);
        if result.takes_the_fast_path() {
            let sent = self.using_item(req, out, item, result, split, now);
            return UseOutcome::Dispatched { result, sent };
        }
        // ---------------------------------------------------------------------------------
        // The general path. The item-use dispatcher runs here too, not only on the fast path:
        // the client calls it on three more lines of this function, and its *tail* is what makes
        // a container open. Without those calls a double-click on a corpse, a chest or a pack
        // in your own inventory sends the use request and opens nothing.
        // ---------------------------------------------------------------------------------
        let uses = ItemUses(
            self.weenie(item)
                .and_then(|w| w.pwd.useability)
                .unwrap_or(0),
        );
        // A traded item cannot be used. The refusal is a literal, not a string-table row.
        if self.weenie(item).is_some_and(|w| w.trade_state == 1) {
            return self.refuse_use(out, item, UseRefusal::BeingTraded);
        }
        // An unwielded item whose least-limited source use requires wielding is refused.
        // Also a literal; all displayed refusals share the same send path.
        let location = self.weenie(item).and_then(|w| w.pwd.location).unwrap_or(0);
        if location == 0
            && uses.least_limited_source_use() & crate::weenie::item_useable::WIELDED != 0
        {
            return self.refuse_use(out, item, UseRefusal::MustBeWielded);
        }
        if uses.is_useable_targeted() {
            // The consume-selection flag — "a targeted use may consume the selection" — is 0 for
            // all three of this client's gestures (see this function's own note), so the
            // selected-object leg that sends the `0x0035` use-with-target request is unreachable
            // from here and the arm that runs records the targeting object and arms
            // `TargetMode::UseTarget`. The mode itself lives in `dereth-client`'s
            // `Interaction`; this reports that it must be armed.
            //
            // The targeted-use path arms the mode and prints
            // `"Choose a target for the %s"` on the very next lines, into the same
            // `0x1A` tail as every refusal: arming and saying so are one act. The *other* leg of
            // this arm — the flag set with no selection — has no producer here; its wording is
            // [`messages::SELECT_YOUR_TARGET_BEFORE_USING`].
            self.targeting_object = item;
            out.emit(Notice::DisplayString {
                feedback: dereth_client_contract::feedback::Feedback::INFORMATION,
                channel: crate::chat::REFUSAL_CHANNEL,
                text: messages::choose_a_target_for(&self.notice_name(item)),
            });
            return UseOutcome::TargetModeArmed;
        }
        if uses.is_useable() {
            if let Some(kind) = self.usage_confirmation(item) {
                out.emit(Notice::UsageConfirmation { object: item, kind });
                return UseOutcome::Confirming(kind);
            }
            self.use_event_and_using_item(req, out, item, split, now);
            // The use announcement.
            // It is *after* `using_item` because the client's is: the tail of `using_item` can
            // open the object as a ground container, and its notices are raised first.
            out.emit(Notice::DisplayString {
                feedback: dereth_client_contract::feedback::Feedback::INFORMATION,
                channel: crate::chat::REFUSAL_CHANNEL,
                text: self.using_notice(item),
            });
            return UseOutcome::UseEventSent;
        }
        // Not useable at all. Retail still runs the using-item path here with both flags 0 —
        // the consume-selection flag is unused — so a container whose useability says
        // `USEABLE_NO` still reaches the tail, and only then does it pick a refusal.
        self.using_item(req, out, item, result, split, now);
        if Some(item) == self.player {
            return self.refuse_use(out, item, UseRefusal::ItIsYou);
        }
        if self
            .weenie(item)
            .is_some_and(|w| w.pwd.bitfield & use_bitfield::DOOR != 0)
        {
            return self.refuse_use(out, item, UseRefusal::Door);
        }
        // `if ((!attackable) || (mode != NONCOMBAT)) { if (attackable && mode !=
        // NONCOMBAT) return; <message>; } else <another message>;` — the only *silent* leg is
        // attackable-and-in-a-stance, where the click was an attack and not a use.
        //
        // `<another message>` is *"To attack %s, click on the dove icon first"* and `<message>`
        // is *"The %s cannot be used"*. The mode is compared with 1, and
        // [`crate::combat::CombatMode::NonCombat`] is 1, so the dove line is the **out of
        // combat** case.
        let attackable = self.object_is_attackable(item);
        let in_combat = self.combat.combat_mode != crate::combat::CombatMode::NonCombat;
        if attackable && in_combat {
            return self.refuse_use(out, item, UseRefusal::AttackingIt);
        }
        if attackable {
            return self.refuse_use(out, item, UseRefusal::AttackDoveIcon);
        }
        self.refuse_use(out, item, UseRefusal::NotUseable)
    }

    /// Display a refusal on the shared inventory-feedback channel `0x1A`.
    ///
    /// One function because the client has one tail: every refusal that carries a text ends in
    /// the same format-and-send block, so there is no per-refusal routing decision to make and
    /// nothing here can put a refusal on a different channel than retail does. The two silent
    /// reasons raise nothing at all, which is [`UseRefusal::text`]'s `None`.
    fn refuse_use(
        &self,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        reason: UseRefusal,
    ) -> UseOutcome {
        if let Some(text) = reason.text(&self.notice_name(item)) {
            out.emit(Notice::DisplayString {
                channel: crate::chat::REFUSAL_CHANNEL,
                text,
                feedback: dereth_client_contract::feedback::Feedback::WARNING,
            });
        }
        UseOutcome::Refused(reason)
    }

    /// The object's name of type 2 — the `%s` of every literal in [`messages`].
    ///
    /// Every one of the nine literals in the use path is formatted with the same name call
    /// (name type 2, flag 0), so the argument is one expression.
    #[must_use]
    pub(super) fn notice_name(&self, item: ObjectId) -> String {
        self.weenie(item).map_or_else(String::new, |w| {
            let material = self.material_name(w.pwd.material_type.unwrap_or(0));
            w.display_name(crate::weenie::NameType::Appropriate, material)
        })
    }

    /// The client's three-bit confirmation ladder, in its order.
    ///
    /// `None` means "send the use request now". The volatile-rare row is behind
    /// a character option: with it clear the item is used
    /// without asking, and the two altars are **not** optional.
    #[must_use]
    pub fn usage_confirmation(&self, item: ObjectId) -> Option<UsageConfirmation> {
        let bits = self.weenie(item).map(|w| w.pwd.bitfield).unwrap_or(0);
        if bits & use_bitfield::PK_ALTAR != 0 {
            return Some(UsageConfirmation::PkAltar);
        }
        if bits & use_bitfield::NPK_ALTAR != 0 {
            return Some(UsageConfirmation::NpkAltar);
        }
        if bits & use_bitfield::VOLATILE_RARE != 0
            && self.player_system.options.confirm_volatile_rare_use()
        {
            return Some(UsageConfirmation::VolatileRare);
        }
        None
    }

    /// Behavior: the `Yes` of all three confirmations, and the
    /// tail of the use path's useable arm. One function, because the client has one callback.
    ///
    /// The use request for `id`, then a busy-count increment, then `using_item`. The
    /// order matters for the last of the three: `using_item`'s tail can open the object as a ground
    /// container **as well as** using it, which is how a chest both fires its use and shows its
    /// contents.
    ///
    /// **The busy-count increment** is one of the six sites named in
    /// `dereth/client/crates/shell/src/app.rs`'s busy-count table. It pairs with the use-done receiver's decrement
    /// (`0x01C7 Item_UseDone`, 137 of them in the corpus); without it every use would leave an
    /// unpaired decrement.
    pub fn confirm_usage(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        split: SplitState,
        now: ServerTime,
    ) {
        self.use_event_and_using_item(req, out, item, split, now);
        // Confirmation prints "Using the %s" with no creature test. Its original
        // direct-scroll path and the ordinary use notice both end at channel 0x1A, so this build
        // emits a notice for both. The message difference is retained: the dialog's Yes
        // never says *"Approaching"*. Unreachable for a creature in practice (the three
        // confirmation bits are item bits), which is presumably why the client's author only
        // wrote one literal here; transcribed rather than unified so that a creature that ever
        // does carry one behaves as retail does.
        let name = self.notice_name(item);
        out.emit(Notice::DisplayString {
            feedback: dereth_client_contract::feedback::Feedback::INFORMATION,
            channel: crate::chat::REFUSAL_CHANNEL,
            text: messages::using_the(&name),
        });
    }

    /// The shared send path for ordinary use and accepted confirmation: send the use request,
    /// increment the busy count, then dispatch the item-use action.
    ///
    /// The two call sites differ only in the notice each of them prints afterwards.
    fn use_event_and_using_item(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        split: SplitState,
        now: ServerTime,
    ) {
        self.attempt_use(req, item);
        self.magic.busy_count += 1;
        let result = self.determine_use_result(item);
        self.using_item(req, out, item, result, split, now);
    }

    /// Choose the use announcement by the creature-type bit `0x10`. See [`messages`].
    #[must_use]
    pub fn using_notice(&self, item: ObjectId) -> String {
        let Some(w) = self.weenie(item) else {
            return messages::using_the("");
        };
        let name = self.notice_name(item);
        if w.pwd.obj_type & dereth_rules::weenie::item_type::CREATURE != 0 {
            messages::approaching(&name)
        } else {
            messages::using_the(&name)
        }
    }

    /// Dispatch the selected use action, then process container opening.
    ///
    /// Returns the client's own "acted" flag: whether anything at all happened.
    ///
    /// The consume-selection flag is not a parameter here because the client's function body
    /// never reads it; only the second-click flag ("this is the second click of a targeted use")
    /// is live, and it gates the switch. Every call site this build has passes 0 for it, so the
    /// switch always runs — and
    /// result classification is re-evaluated inside this function in retail, which is why `result` is
    /// passed in rather than recomputed: the two are the same value in every reachable path and
    /// passing it makes that visible.
    ///
    /// **The tail is unconditional.** It runs whatever the switch did, including nothing:
    ///
    /// ```text
    /// owned by the player and a container                   open-contained-container notice
    /// useable, not targeted, a container, not owned         attempt_set_ground_object
    /// ```
    ///
    /// The first opens one of your own packs onto the backpack grid; the second is how a **corpse
    /// or a chest becomes the open ground object**, which is what `determine_use_result`'s first
    /// gate exempts and therefore what makes loot inside it double-clickable into your pack. The
    /// second line is the ground-object field's only production writer.
    pub(super) fn using_item(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        result: UseResult,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        // A zero id or an id with no weenie returns false.
        if item.0 == 0 || self.weenie(item).is_none() {
            return false;
        }
        let mut acted = self.using_item_switch(req, out, item, result, split, now);
        // ---- the tail ----
        let uses = ItemUses(
            self.weenie(item)
                .and_then(|w| w.pwd.useability)
                .unwrap_or(0),
        );
        let container = self.weenie(item).is_some_and(Weenie::is_container);
        let owned = self.is_owned_by_player(item);
        if owned && container {
            out.emit(Notice::OpenContainedContainer(item));
            acted = true;
        }
        if uses.is_useable() && !uses.is_useable_targeted() && container && !owned {
            // "acted" is set **regardless of what the ground-object attempt answered**, which
            // is the client's: the gesture was understood even when the object refuses to open.
            self.attempt_set_ground_object(req, out, item, now);
            acted = true;
        }
        acted
    }

    /// `using_item`'s switch on the use result, the eight-arm half.
    fn using_item_switch(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        result: UseResult,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        use dereth_rules::slots::SlotSide;
        match result {
            UseResult::PlaceInBackpack => self.place_in_backpack(req, out, item, false, split, now),
            // **The double-click's own `auto_wield` arguments, which are not `AutoSort`'s.**
            //
            // `using_item`'s cases 3 and 8 take the same path, at the first and last entries of
            // the seven-entry switch:
            //
            // ```text
            // case 3: auto_wield(id, side = 2, quiet = 0, unblock = 1, allow_sort = 0, other_side = 1)
            // case 8: auto_wield(id, side = 1, quiet = 0, unblock = 1, allow_sort = 0, other_side = 1)
            // ```
            //
            // The right side is 2 and the left side is 1, matching the slot-selection code.
            //
            // The auto-sort wield arguments are `auto_wield(id, RIGHT, 1, 0, 0, 1)`. Three of
            // the six arguments differ, and **`unblock` is the one the player feels**: with it 0
            // the walk records the blocker, prints *"You're already wielding …"* and stops, so a
            // double-click on a second sword would do nothing while dragging the same sword onto
            // the ready slot moves the first one to the pack and equips it. Two call sites of one
            // function, two argument sets; do not merge them.
            //
            // `unblock = 1` also suppresses all fifteen per-slot refusal strings,
            // so double-click is **silent up to the point
            // where it says "Moving <blocker> to your backpack"** — which is on channel `0x1A`,
            // in bright red. The red line the drop path prints is that notice and it is
            // retail's own; it is not a refusal.
            UseResult::WieldRight | UseResult::WieldLeft => {
                let side = if result == UseResult::WieldLeft {
                    SlotSide::Left
                } else {
                    SlotSide::Right
                };
                self.auto_wield(req, out, item, side, false, true, true, split, now)
            }
            // `auto_sort(id, allow_wield = 1, quiet = 0)`: the whole auto-sort function, not
            // two of its five steps with `quiet = 1`, so a double-click on a blocked piece of
            // armour speaks rather than being a silent no-op.
            UseResult::AutoSort => self.auto_sort(req, out, item, true, false, split, now),
            // ---- arms 5, 6 and 7 ----
            //
            // Behavior: The **only one of
            // the three that puts bytes on the wire**: without it double-clicking another player
            // does nothing at all.
            UseResult::Trade => self.attempt_to_open_trade_negotiations(req, out, item),
            // Opening salvage emits a notice and nothing else;
            // "acted" is set unconditionally in the client.
            UseResult::Salvage => {
                out.emit(Notice::OpenSalvagePanel(item));
                true
            }
            // Starting a board game does not set the action-result flag
            // on this arm — it is the one `case` with no assignment — so a game board that is not
            // also a container makes `using_item` answer `false`. Transcribed, not tidied.
            UseResult::BeginGame => {
                out.emit(Notice::BeginGame(item));
                false
            }
            UseResult::Nothing | UseResult::Useable => false,
        }
    }

    /// Open trade negotiations: item-use result 5.
    ///
    /// Refuse in any combat stance with retail's peace-mode message,
    /// and otherwise send `0x01F6 Trade_OpenTradeNegotiations(partner)`. The non-zero partner guard
    /// is the client's own and is why `false` is possible with no message.
    pub fn attempt_to_open_trade_negotiations(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        partner: ObjectId,
    ) -> bool {
        if self.combat.combat_mode != crate::combat::CombatMode::NonCombat {
            // Retail's text: compare the mode with noncombat (1), then display
            // "You need to be in peace mode to trade." on channel 0x1A.
            out.emit(Notice::DisplayString {
                feedback: dereth_client_contract::feedback::Feedback::WARNING,
                channel: crate::trade::TRADE_MESSAGE_CHANNEL,
                text: crate::trade::messages::PEACE_MODE.to_string(),
            });
            return false;
        }
        if partner.0 == 0 {
            return false;
        }
        req.send(crate::Request::OpenTradeNegotiations(
            dereth_protocol::trade::TradeOpenTradeNegotiations { partner },
        ));
        true
    }

    /// Behavior: **how a corpse becomes lootable.**
    ///
    /// The gate is `bitfield & OPENABLE`, bit 0, and nothing else: an object that is a container
    /// and useable but not openable is refused, and a **creature** is refused silently while
    /// anything else gets *"The %s is locked"* with
    /// the object's name of type 2 -- see [`messages::is_locked`].
    pub fn attempt_set_ground_object(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        now: ServerTime,
    ) -> GroundObjectResult {
        // A readiness refusal is always displayed by this caller.
        if let Err(e) = self.ready_for_inventory_request() {
            self.refuse(out, false, e);
            return GroundObjectResult::Busy;
        }
        let Some(w) = self.weenie(item) else {
            // A retail literal, not a string-table row, so it is reproduced exactly.
            self.refuse(
                out,
                false,
                "The object has no representation on the client.",
            );
            return GroundObjectResult::NoRepresentation;
        };
        if w.pwd.bitfield & dereth_rules::weenie::bitfield::OPENABLE == 0 {
            let creature = w.is_creature();
            if !creature {
                // The client formats a literal of its own here. Measured live: the shard answers
                // a locked-chest use with the sound and `UseDone(0)` only, so this line is the
                // only place *"The Aluvian Pathwarden Chest is locked"* can come
                // from. `%s` is the object's name of type 2, as for every use-path literal.
                let name = self.notice_name(item);
                self.refuse(out, false, &messages::is_locked(&name));
            }
            return GroundObjectResult::NotOpenable { creature };
        }
        if self.set_ground_object(req, out, Some(item), true, now) {
            GroundObjectResult::Opened
        } else {
            GroundObjectResult::AlreadyOpen
        }
    }

    /// Replace the open ground object, optionally notifying the server that viewing ended.
    ///
    /// Returns whether anything changed — the client's own "is this a different ground object"
    /// guard, which covers the **whole** body.
    ///
    /// The order is the client's and each step matters:
    ///
    /// 1. an open vendor is closed (vendor id cleared, close-vendor notice with `false`);
    /// 2. the **old** container's contents go on the destruction queue and
    ///    [`Notice::SetGroundObject`]`(0)` closes the panel;
    /// 3. the no-longer-viewing flag puts `0x0195` on the wire for the old container — the caller
    ///    decides, with `true` for a local change and
    ///    `false` from the `0x0052` handler, because in the second case the server is the one
    ///    ending it;
    /// 4. a selection that lives **inside** the old container is cleared, so the toolbar does not
    ///    keep a meter on an item about to be destroyed;
    /// 5. only then is the new id assigned — to both `ground_object` and
    ///    `requested_ground_object`;
    /// 6. a corpse is recorded in the opened-corpses set.
    ///
    /// **The panel does not open here.** This setter raises only the *closing* notice; the
    /// opening one comes from the contents handler when the server answers `0x0196`. That
    /// asymmetry is the client's and it is what makes the ground panel show real contents rather
    /// than an empty frame.
    pub fn set_ground_object(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        id: Option<ObjectId>,
        send_no_longer_viewing: bool,
        now: ServerTime,
    ) -> bool {
        let id = id.filter(|i| i.0 != 0);
        if self.ground_object == id {
            return false;
        }
        // Close the vendor before tearing down the old ground object. The
        // outer equality guard above also means selecting the already-open ground object does not
        // close a vendor.
        self.close_vendor(out);
        if let Some(old) = self.ground_object {
            self.add_contents_to_destruction_queue(old, now);
            out.emit(Notice::SetGroundObject(ObjectId(0)));
            // The client's first act is
            // unregistering the ground object's range handler, before touching the lists.
            // The panel consumes the closing notice; the model owns range registrations.
            self.object_range_checks
                .unregister(crate::range::RangeHandler::ExternalContainer, old);
            if send_no_longer_viewing {
                req.send(crate::Request::NoLongerViewingContents(
                    dereth_protocol::objects::InventoryNoLongerViewingContents { container: old },
                ));
            }
            // Still reading the **old** ground object: the client assigns after this block.
            if let Some(sel) = self.selected.filter(|s| s.0 != 0 && *s != old) {
                if self.is_owned_by_object(sel, old) {
                    self.set_selected_object(None, false, out);
                }
            }
        }
        self.ground_object = id;
        self.requested_ground_object = id;
        if let Some(new) = id {
            if self.weenie(new).is_some_and(Weenie::is_corpse) {
                // Remember that this corpse has been opened during the session.
                self.opened_corpses.insert(new);
            }
        }
        true
    }

    /// Whether the corpse was opened during this session.
    ///
    /// The selection-type filter's unopened-corpse arm
    /// is this function's only reader, exactly as the client has it —
    /// the corpse test and then "not already opened".
    ///
    /// Four input actions reach that arm —
    /// `SelectionUseClosestUnopenedCorpse` (`0x1000003E`), `SelectionUseNextUnopenedCorpse`
    /// (`0x1000003F`), `SelectionClosestUnopenedCorpse` (`0x10000121`) and
    /// `SelectionNextUnopenedCorpse` (`0x10000122`); the first two also use the winner.
    #[must_use]
    pub fn has_corpse_been_opened(&self, id: ObjectId) -> bool {
        self.opened_corpses.contains(&id)
    }

    /// Behavior: **the reply that opens the ground panel.**
    ///
    /// `0x0196 Item_OnViewContents`'s whole handler. The second half is the one that makes the
    /// panel appear:
    ///
    /// ```text
    /// view_object_contents(container, profiles)
    /// remove_contents_from_destruction_queue(container)
    /// if container is the requested ground object, and the last request was not
    ///    a pick-up request of this same container:
    ///     raise the ground-object notice; record_response(container)
    /// ```
    ///
    /// The pick-up exclusion is the interesting line: a `0x0196` that is the answer to a
    /// *pickup* out of this very container must not re-open it, or every item taken from a corpse
    /// would re-raise the panel. Returns whether the panel was opened.
    pub fn on_view_contents(
        &mut self,
        container: ObjectId,
        profiles: &[ContentProfile],
        out: &mut dyn NoticeSink,
        now: ServerTime,
    ) -> bool {
        self.view_object_contents(container, profiles, out);
        if self.weenie(container).is_some() {
            self.remove_contents_from_destruction_queue(container);
        }
        if self.requested_ground_object != Some(container) {
            return false;
        }
        let answering_a_pickup = self.request_lock.object == Some(container)
            && self.request_lock.pending == crate::inventory::requests::InventoryRequest::PickUp;
        if answering_a_pickup {
            return false;
        }
        out.emit(Notice::SetGroundObject(container));
        // Range-check registrant #5: opening the panel registers a range check at the
        // container's use radius. See [`crate::range`].
        self.register_ground_object_range_check(container, now);
        self.record_response(container);
        true
    }

    /// Behavior: clear the lock **only** when the answer is
    /// about the object it is holding.
    pub fn record_response(&mut self, id: ObjectId) {
        self.request_lock.clear_if_matches(id);
    }

    /// The inbound dispatcher's `case 0x52` — `0x0052 Item_StopViewingObjectContents`, whole.
    ///
    /// ```text
    /// if id is the ground object: set_ground_object(0, false); record_response(id)
    /// stop_viewing_object_contents(id)
    /// ```
    ///
    /// The `false` matters: the **server** is the one ending the view here, so replying `0x0195`
    /// would be telling it what it just told us. Returns whether the ground container was the one
    /// that closed.
    pub fn handle_stop_viewing_object_contents(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        id: ObjectId,
        now: ServerTime,
    ) -> bool {
        let was_ground = self.ground_object == Some(id);
        if was_ground {
            self.set_ground_object(req, out, None, false, now);
            self.record_response(id);
        }
        self.stop_viewing_object_contents(id, out);
        was_ground
    }

    /// The client's own wield leg: `auto_wield(id, SlotSide::Right, 1, 0, 0, 1)`
    /// — quiet, **no unblock**, and `other_side = 1`
    /// so it may take the other wrist or ring.
    ///
    /// This goes through the whole auto-wield function rather than
    /// through the slot walk alone. `unblock = false` is the client's: a shift-click that finds
    /// every slot full prints the walk's messages and stops. Only the paper doll unwields for you.
    pub(crate) fn wield_to_first_free(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        side: dereth_rules::slots::SlotSide,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        self.auto_wield(req, out, item, side, true, false, true, split, now)
    }

    /// Place an object in the backpack, including pickup from the ground.
    ///
    /// The destination is the open container unless the caller forces the main pack or the player's
    /// `MainPackPreferred` option (options2 bit 14) is set, in which case it is the player.
    /// [`Notice::ShowPendingInPlayer`] ghosts the icon before the request and the end-pending
    /// notice plus clearing the waiting state undo it when the request is refused —
    /// the client predicts nothing, so a refusal must be un-ghosted by hand.
    pub fn place_in_backpack(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        force_main_pack: bool,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        if item.0 == 0 {
            return false;
        }
        let Some(player) = self.player else {
            return false;
        };
        out.emit(Notice::ShowPendingInPlayer(item));
        let container = if force_main_pack || self.player_system.options.main_pack_preferred() {
            player
        } else {
            self.open_container.unwrap_or(player)
        };
        if self
            .attempt_to_place_in_container(req, out, item, player, container, true, 0, split, now)
        {
            return true;
        }
        // Clear the waiting state through the private helper in this
        // module's parent, not the public re-export.
        self.set_waiting(item, false);
        out.emit(Notice::EndPendingInPlayer);
        false
    }

    /// Attempt to place an item under an owner, optionally in a requested container.
    ///
    /// The order is the client's and each step can change the destination:
    ///
    /// 1. the inventory lock, then legality checks on the owner and item;
    /// 2. a requested container that is neither openable nor the player itself is **discarded** — the id
    ///    becomes 0 and the spill in step 4 picks somewhere else;
    /// 3. `attempt_auto_merge` over the *owner's* exhaustive list, which is why picking up coins
    ///    sends the `0x0054` stack-merge request and not `0x0019`;
    /// 4. the spill: the requested container, the owner's main pack, then each side pack
    ///    (the spill-target helper);
    /// 5. whole stack → `attempt_put_in_container`, part of one → `attempt_split_to_container`.
    #[allow(clippy::too_many_arguments)]
    pub fn attempt_to_place_in_container(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        owner: ObjectId,
        container: ObjectId,
        auto_merge: bool,
        place: u32,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        if let Err(e) = self.ready_for_inventory_request() {
            self.refuse(out, false, e);
            return false;
        }
        if let Err(text) = self.place_in_container_item_legal(item) {
            // `quiet = 0`, so every one of the six refusal arms speaks.
            self.refuse(out, false, &text);
            return false;
        }
        if let Err(text) = self.place_in_container_owner_legal(item, owner) {
            self.refuse(out, false, &text);
            return false;
        }
        // Step 2: the destination has to survive three tests or it is **discarded** -- the id
        // becomes 0 and the spill in step 4 picks somewhere else. It must be openable or be the
        // player himself, it must not be in a trade, and it must pass the hook rules.
        let mut container = container;
        if container.0 != 0 && container != item {
            if let Some(c) = self.weenie(container) {
                let openable = c.pwd.bitfield & dereth_rules::weenie::bitfield::OPENABLE != 0;
                let tradeable = c.trade_state != 1;
                let keep = (openable || Some(container) == self.player)
                    && tradeable
                    && self.check_hook_status(item, container).0;
                if !keep {
                    container = ObjectId(0);
                }
            }
        }
        if auto_merge && self.attempt_auto_merge(req, out, item, owner, split, now) {
            return true;
        }
        if self.weenie(item).is_none() || self.weenie(owner).is_none() {
            return false;
        }
        let Some(target) = self.spill_target(item, owner, container, split) else {
            // Once every spill
            // destination is full, retail classifies the owner and the incoming object and emits
            // one of four local-error lines. Only the player/non-container arm asks for the
            // appropriate name with the alias flag set, which the name lookup
            // turns into the literal "Backpack" only when the owner is the local player.
            // The other three calls pass alias=false and retain the normal material-aware name.
            let item_is_container = self
                .weenie(item)
                .is_some_and(crate::weenie::Weenie::is_container);
            let owner_is_player = self
                .weenie(owner)
                .is_some_and(crate::weenie::Weenie::is_player);
            let owner_name = if owner_is_player && !item_is_container && self.player == Some(owner)
            {
                "Backpack".to_owned()
            } else {
                self.notice_name(owner)
            };
            let text = match (owner_is_player, item_is_container) {
                (true, true) => format!("{owner_name} can carry no more containers!"),
                (true, false) => format!("{owner_name} is completely full!"),
                (false, true) => format!("The {owner_name} can fit no more containers!"),
                (false, false) => format!("The {owner_name} is completely full!"),
            };
            self.refuse(out, false, &text);
            return false;
        };
        if split.is_whole_stack() {
            self.attempt_put_in_container(req, out, item, target, place, now, false)
                .is_ok()
        } else {
            self.attempt_split_to_container(
                req,
                out,
                item,
                target,
                place,
                split.split_size,
                now,
                false,
            )
            .is_ok()
        }
    }

    /// Behavior: how many of a stack a gesture moves.
    ///
    /// The selected item is the one the splitter is aimed at, so it moves the selected amount,
    /// and anything else moves its whole stack. `max(stack size, 1)` is the client's, and it is
    /// what makes a non-stackable object move as one.
    #[must_use]
    pub fn object_split_size(&self, item: ObjectId, split: SplitState) -> u32 {
        if self.selected == Some(item) {
            return split.split_size;
        }
        self.weenie(item)
            .map_or(1, |w| u32::from(w.pwd.stack_size.unwrap_or(0).max(1)))
    }

    /// Offer a dragged item to a vendor, opening the shop if needed.
    ///
    /// The shop's `attempt_open_vendor` and `attempt_sale_object` are read by
    /// the vendor-advertisement handler; this is their only writer, and it is reachable only
    /// from below.
    ///
    /// If the vendor is already open the item goes straight onto the sell list; otherwise the
    /// vendor is *used* (which is what opens the shop) and the item is parked until `0x0062
    /// Vendor_VendorInfo` comes back naming that merchant.
    pub fn attempt_sell_to_vendor(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        vendor: ObjectId,
        item: ObjectId,
        split: SplitState,
        now: ServerTime,
    ) {
        if self.weenie(item).is_none() || !self.is_owned_by_player(item) {
            return;
        }
        if self.shop.vendor_id == Some(vendor) {
            // An already-open shop can accept the item immediately.
            self.add_item_to_sell(item, out);
            return;
        }
        self.use_object(req, out, vendor, split, now);
        self.shop.attempt_open_vendor = Some(vendor);
        self.shop.attempt_sale_object = Some(item);
    }

    /// Offer a dragged item to a trade partner, opening negotiations if needed.
    ///
    /// The same deferred-open pattern as vendor sales: the trade state's `attempt_to_player` /
    /// `attempt_object` are read by the negotiation-start handler's deferred half, and this is
    /// their writer.
    ///
    /// With a negotiation already open to this partner the item is offered immediately; with one
    /// open to somebody else, the client refuses with retail's already-trading message.
    /// Otherwise the partner is
    /// *used*, which is what asks them to open the window, and the item is parked.
    pub fn attempt_to_trade_item(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        partner: ObjectId,
        item: ObjectId,
        split: SplitState,
        now: ServerTime,
    ) {
        if self.weenie(item).is_none() || !self.is_owned_by_player(item) {
            return;
        }
        if self.trade.partner.0 != 0 {
            if self.trade.partner == partner {
                out.emit(Notice::TradeAnItemForDummies(item));
            } else {
                // Retail's already-trading literal, an ordinary wide literal sent on channel 0x1A.
                out.emit(Notice::DisplayString {
                    feedback: dereth_client_contract::feedback::Feedback::WARNING,
                    channel: crate::trade::TRADE_MESSAGE_CHANNEL,
                    text: crate::trade::messages::ALREADY_TRADING.to_string(),
                });
            }
            return;
        }
        self.use_object(req, out, partner, split, now);
        self.trade.attempt_to_player = partner;
        self.trade.attempt_object = item;
    }

    /// Handle a drop into the 3-D viewport, including giving an object to an NPC.
    ///
    /// Starting the drop sets the drop search reason and parks the
    /// dragged id; the 3-D pick answers with whatever was under the cursor; and
    /// the viewport's pick-result handler calls this with the dragged id, found id and ground
    /// placement enabled, un-ghosting the item when it returns false. The viewport receives the drop,
    /// and the request it raises is `0x00CD Inventory_GiveObjectRequest`.
    ///
    /// The client's order, and every step of it can change the answer:
    ///
    /// ```text
    /// not ready for an inventory request (non-quiet)    -> false
    /// no weenie for item                                -> false
    /// onto is the player                                -> place_in_backpack(item, false)
    /// item not owned by the player    "You must first pick up the %s"                  -> false
    /// item is being traded            "You are trading the %s, it cannot be dropped"   -> false
    /// onto is non-zero:
    ///     attempt_merge(item, onto, quiet 0) succeeded                              -> its answer
    ///     onto has a weenie:
    ///         BF_VENDOR set: whole stack -> attempt_sell_to_vendor(onto, item)     -> false
    ///                        else "You must split the stack before selling it."    -> false
    ///         drag-onto-player-opens-trade option and onto is a player
    ///                        -> attempt_to_trade_item(onto, item)                  -> false
    ///         type == 0x10   -> attempt_give(item, onto, object_split_size(item))  -> true
    ///         a container:   BF_OPENABLE clear  "The %s is locked"                 -> false
    ///                        not the ground object  "You must open the %s first"   -> false
    ///                        r = attempt_to_place_in_container(item, onto, 0, 1, 0)
    ///         ground not allowed  "Cannot give %s to %s"                           -> false
    ///     r non-zero                                                               -> r
    /// ground not allowed                                                           -> false
    /// player not on the ground  "You cannot do that in mid air"                    -> false
    /// split size < max split size -> attempt_split_to_3d(item, split size)         -> true
    /// item not already in the 3-D view -> attempt_put_in_3d(item)                  -> true
    /// "Move cancelled"                                                             -> false
    /// ```
    ///
    /// **The give test is `type == 0x10`, an equality and not a mask**, so an object whose
    /// type carries the creature bit and another bit does not take this arm.
    ///
    /// Retail's refusal literals are reproduced below.
    /// The `Cannot give %s to %s` arm is `allow_ground == false`, which the viewport drop never is.
    ///
    /// The caller supplies the player's on-ground state
    /// for the same reason `set_combat_mode`'s readiness is supplied — this crate has no physics.
    ///
    /// Returns the client's own integer answer as a `bool`; `false` is the caller's cue to
    /// clear the item's waiting state.
    #[allow(clippy::too_many_arguments)]
    pub fn attempt_place_in_3d(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        onto: Option<ObjectId>,
        allow_ground: bool,
        player_on_ground: bool,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        use {crate::weenie::PositionState, dereth_rules::weenie::bitfield};

        if let Err(e) = self.ready_for_inventory_request() {
            self.refuse(out, false, e);
            return false;
        }
        let Some(w) = self.weenie(item) else {
            return false;
        };
        let item_name = self.notice_name(item);
        let trading = w.trade_state != 0;

        // dropping something onto **yourself** is the pickup, whatever else is true.
        if onto.is_some() && onto == self.player {
            return self.place_in_backpack(req, out, item, false, split, now);
        }
        if !self.is_owned_by_player(item) {
            self.refuse(
                out,
                false,
                &format!("You must first pick up the {item_name}"),
            );
            return false;
        }
        if trading {
            self.refuse(
                out,
                false,
                &format!("You are trading the {item_name}, it cannot be dropped"),
            );
            return false;
        }

        let mut placed = false;
        if let Some(target) = onto.filter(|t| t.0 != 0) {
            // The merge attempt onto the target — the quiet argument is **0** here where the item
            // list's own call site passes 1.
            if self.item_holder_attempt_merge(req, out, item, target, split, now) {
                return true;
            }
            if let Some(t) = self.weenie(target) {
                let target_name = self.notice_name(target);
                let is_vendor = t.pwd.bitfield & bitfield::VENDOR != 0;
                let is_player = t.is_player();
                let ty = t.inq_type();
                let container = t.is_container();
                let openable = t.pwd.bitfield & bitfield::OPENABLE != 0;

                if is_vendor {
                    if split.is_whole_stack() {
                        self.attempt_sell_to_vendor(req, out, target, item, split, now);
                    } else {
                        self.refuse(out, false, "You must split the stack before selling it.");
                    }
                    return false;
                }
                if self
                    .player_system
                    .options
                    .get(crate::player::options::option::DRAG_ITEM_ON_PLAYER_OPENS_SECURE_TRADE)
                    && is_player
                {
                    self.attempt_to_trade_item(req, out, target, item, split, now);
                    return false;
                }
                // Type `0x10` (creature) — the give.
                if ty == dereth_rules::weenie::item_type::CREATURE {
                    let amount = self.object_split_size(item, split);
                    return self
                        .attempt_give(req, out, item, target, amount, now, false)
                        .is_ok();
                }
                if container {
                    if !openable {
                        self.refuse(out, false, &format!("The {target_name} is locked"));
                        return false;
                    }
                    if self.ground_object != Some(target) {
                        self.refuse(
                            out,
                            false,
                            &format!("You must open the {target_name} first"),
                        );
                        return false;
                    }
                    placed = self.attempt_to_place_in_container(
                        req,
                        out,
                        item,
                        target,
                        ObjectId(0),
                        true,
                        0,
                        split,
                        now,
                    );
                }
                if !allow_ground {
                    self.refuse(
                        out,
                        false,
                        &format!("Cannot give {item_name} to {target_name}"),
                    );
                    return false;
                }
            }
            if placed {
                return true;
            }
        }
        if !allow_ground {
            return false;
        }
        if !player_on_ground {
            self.refuse(out, false, "You cannot do that in mid air");
            return false;
        }
        if !split.is_whole_stack() {
            return self
                .attempt_split_to_3d(req, out, item, split.split_size, now, false)
                .is_ok();
        }
        // Not yet in the 3-D view: put it there; otherwise "Move
        // cancelled" -- dropping something that is *already* on the ground onto the ground is not
        // a move, it is a cancelled one.
        if self.weenie(item).map(|w| w.current_state) == Some(PositionState::In3dView) {
            self.refuse(out, false, "Move cancelled");
            return false;
        }
        self.attempt_put_in_3d(req, out, item, now, false).is_ok()
    }

    /// Check whether a housing hook accepts the item and whether the house is unowned.
    ///
    /// Returns two flags: whether the container will take the item, and the value the client
    /// writes through its third argument -- "this hook is on a house nobody owns".
    ///
    /// A container that is **not a hook** -- `hook_type == 0` or `hook_item_types == 0`, which is
    /// exactly [`crate::Weenie::is_hook`]'s own test -- returns `true` immediately. That is why
    /// leaving this check out changes nothing outside housing, and why it is cheap to have it: a
    /// hook takes an item only when the item's own `hook_type` overlaps the hook's **and** the
    /// item's type overlaps `hook_item_types`.
    #[must_use]
    pub fn check_hook_status(&self, item: ObjectId, container: ObjectId) -> (bool, bool) {
        let (Some(i), Some(c)) = (self.weenie(item), self.weenie(container)) else {
            return (false, false);
        };
        let hook_type = c.pwd.hook_type.unwrap_or(0);
        let hook_item_types = c.pwd.hook_item_types.unwrap_or(0);
        if hook_type == 0 || hook_item_types == 0 {
            return (true, false);
        }
        if c.pwd.house_owner_iid.unwrap_or_default().0 == 0 {
            return (false, true);
        }
        let item_hook = i.pwd.hook_type.unwrap_or(0);
        if item_hook != 0 && hook_type & item_hook != 0 {
            return (i.inq_type() & hook_item_types != 0, false);
        }
        (false, false)
    }

    /// Validate the destination owner before selecting a container.
    ///
    /// This validates `attempt_to_place_in_container`'s *owner* argument before that function looks at
    /// its separate requested-container argument. The latter can be discarded and replaced by the
    /// spill; an illegal owner stops the attempt instead. The native caller passes `quiet = false`,
    /// so every error returned here is displayed on inventory-feedback channel `0x1A` by the
    /// caller.
    fn place_in_container_owner_legal(
        &self,
        item: ObjectId,
        owner: ObjectId,
    ) -> Result<(), String> {
        let Some(container) = self.weenie(owner) else {
            return Err("The destination container is not valid!".to_owned());
        };
        if item == owner {
            return Err("You cannot place an object within itself!".to_owned());
        }
        if container.pwd.bitfield & dereth_rules::weenie::bitfield::OPENABLE == 0
            && Some(owner) != self.player
        {
            return Err(format!("The {} is locked", self.notice_name(owner)));
        }
        if container.trade_state == 1 {
            return Err(format!("The {} is being traded", self.notice_name(owner)));
        }
        let (legal, unowned_house) = self.check_hook_status(item, owner);
        if legal {
            return Ok(());
        }
        let mut text = "That item cannot be placed on the hook.".to_owned();
        if unowned_house {
            text.push_str(" You must own the house to manipulate the hook.");
        }
        Err(text)
    }

    /// Validate whether the item may be placed in a container.
    ///
    /// # Errors
    /// The refusal string, retail's own literal. Every one reaches the channel-`0x1A` display
    /// because container placement calls this with `quiet = 0`. Note the two
    /// *different* spellings of the same idea, which are the client's own:
    /// `The %s cannot be picked up!` for a `STUCK` object and `The %s can't be picked up!` for a
    /// pack with its own container capacity.
    ///
    /// The drag gate runs the same six checks with `quiet = 1`, so it is silent while this
    /// placement path displays the refusal.
    pub fn place_in_container_item_legal(&self, item: ObjectId) -> Result<(), String> {
        let Some(w) = self.weenie(item) else {
            return Err(request_messages::ITEM_NOT_VALID.to_string());
        };
        if Some(item) == self.player {
            return Err("You cannot place yourself within a container!".to_string());
        }
        // `type == ITEM_TYPE_CREATURE` **exactly**, not a mask test.
        if w.inq_type() == item_type::CREATURE {
            return Err("You cannot pick up creatures!".to_string());
        }
        let stuck = w.pwd.bitfield & dereth_rules::weenie::bitfield::STUCK != 0;
        let contained = w.pwd.container_id.unwrap_or_default().0 != 0;
        let wielded = w.pwd.wielder_id.unwrap_or_default().0 != 0;
        if stuck && !contained && !wielded {
            // A stuck, loose object cannot be picked up.
            return Err(format!(
                "The {} cannot be picked up!",
                self.notice_name(item)
            ));
        }
        if !self.is_owned_by_player(item) && w.pwd.location.unwrap_or(0) != 0 {
            // An object wielded by someone else cannot be taken.
            return Err(format!(
                "The {} is being wielded by someone else!",
                self.notice_name(item)
            ));
        }
        if w.is_container() && w.pwd.containers_capacity.unwrap_or(0) != 0 {
            // This refusal deliberately uses the `can't` spelling.
            return Err(format!(
                "The {} can't be picked up!",
                self.notice_name(item)
            ));
        }
        Ok(())
    }

    /// Find the first legal automatic-merge destination and send the merge request.
    ///
    /// The scan is over the **owner's** exhaustive contained-items list — the main pack then each
    /// side pack — and the first candidate that is a legal merge target *and* has room for the
    /// whole split wins. The amount is the split size for the selected object and the whole source
    /// stack otherwise, clamped to the destination's remaining room; a stack size of 0 counts as 1.
    fn attempt_auto_merge(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        owner: ObjectId,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        let Some(src) = self.weenie(item) else {
            return false;
        };
        if src.pwd.max_stack_size.unwrap_or(0) <= 1 || self.weenie(owner).is_none() {
            return false;
        }
        let src_stack = u32::from(src.pwd.stack_size.unwrap_or(0).max(1));
        for cand in self.exhaustive_contained_items(owner) {
            if cand == item || self.weenie(cand).is_none() {
                continue;
            }
            if self.is_merge_attempt_legal(item, cand).is_err() {
                continue;
            }
            let Some(d) = self.weenie(cand) else { continue };
            let room = u32::from(d.pwd.max_stack_size.unwrap_or(0))
                .saturating_sub(u32::from(d.pwd.stack_size.unwrap_or(0).max(1)));
            if split.split_size > room {
                continue;
            }
            let want = if Some(item) == self.selected {
                split.split_size
            } else {
                src_stack
            };
            let amount = want.min(room);
            let sent = self
                .attempt_merge(req, out, item, cand, amount, now, false)
                .is_ok();
            if sent {
                self.set_selected_object(Some(cand), false, out);
            }
            return sent;
        }
        false
    }
}
