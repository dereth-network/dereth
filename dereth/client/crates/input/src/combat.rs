//! The combat input maps, and the one that is registered at a time.
//!
//! The client owns four combat input maps and they are **not** all live together:
//! `Combat` (`0x10000002`) is registered once per character session and stays, while exactly one
//! of `MeleeCombat` / `MissileCombat` / `MagicCombat` is swapped in as the combat mode changes —
//! and in peace mode **none** of the three is registered.
//!
//! ```text
//! the combat system's input-map registration, given (new mode, old mode)
//!   switch (old mode) { 2 MELEE -> 0x10000003, 4 MISSILE -> 0x10000004, 8 MAGIC -> 0x10000005 }
//!   unregister_input_map(that map, callback)
//!   register_input_map(0x10000002, 1000, callback)        ; unconditionally, every call
//!   switch (new mode) { the same 2 / 4 / 8 ladder }
//!   register_input_map(that map, 1000, callback)
//! ```
//!
//! \[verified\] Three things in it are easy to get wrong:
//!
//! * the **unregister** is driven by the *previous* mode and the **register** by the *new* one —
//!   the combat-mode setter passes its new combat mode first and the old one second;
//! * `NONCOMBAT_COMBAT_MODE` (1) and `UNDEF_COMBAT_MODE` (0) match no rung of either ladder, so
//!   entering peace mode unregisters the old map and registers **nothing** in its place;
//! * the middle step re-registers `0x10000002` on **every** call, and it is a no-op every time
//!   after the first: registration rejects an exact `{map, callback, priority}` triple, and the
//!   session-start handler has already put that triple in with the same callback and the same
//!   priority. So `Combat` does **not** move to the front of the band on a mode change; the mode
//!   map, registered last at priority 1000, does.
//!
//! That last point is what decides the walk. Registration puts an equal-priority
//! newcomer in **front**, so from the first mode change onwards the live combat map is the first
//! gameplay-priority (1000) map walked -- ahead of `QuickslotCommands`, which the quickslot system
//! registered at session start.

use crate::InputMapId;

/// `COMBAT_MODE`, from the client's own enum. The three fighting modes are single bits.
pub mod mode {
    pub const UNDEF: u32 = 0;
    pub const NONCOMBAT: u32 = 1;
    pub const MELEE: u32 = 2;
    pub const MISSILE: u32 = 4;
    pub const MAGIC: u32 = 8;
}

/// `Combat` -- `CombatToggleCombat` and the power-bar keys. Registered by the combat system at
/// the gameplay priority (1000) and never
/// unregistered until the session ends.
pub const COMBAT_MAP: InputMapId = InputMapId(0x1000_0002);
/// `MeleeCombat` — the attack heights and the attack-power keys.
pub const MELEE_COMBAT_MAP: InputMapId = InputMapId(0x1000_0003);
/// `MissileCombat` — the aim heights and the missile-accuracy keys.
pub const MISSILE_COMBAT_MAP: InputMapId = InputMapId(0x1000_0004);
/// `MagicCombat` — the spell list, the spell tabs and `UseSpellSlot_1`…`_9` on `DIK_1`…`DIK_9`.
pub const MAGIC_COMBAT_MAP: InputMapId = InputMapId(0x1000_0005);

/// The three maps that are mutually exclusive, in mode order. **Never register more than one.**
pub const MODE_COMBAT_MAPS: [InputMapId; 3] =
    [MELEE_COMBAT_MAP, MISSILE_COMBAT_MAP, MAGIC_COMBAT_MAP];

/// The client's ladder: which combat map a `COMBAT_MODE`
/// selects, or `None` for peace and for the undefined mode.
#[must_use]
pub const fn input_map_for_combat_mode(mode: u32) -> Option<InputMapId> {
    match mode {
        mode::MELEE => Some(MELEE_COMBAT_MAP),
        mode::MISSILE => Some(MISSILE_COMBAT_MAP),
        mode::MAGIC => Some(MAGIC_COMBAT_MAP),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ladder, as literals, against the switch in this module's own documentation.
    ///
    /// A test that reads the map ids back through the same symbols it wrote them through cannot
    /// detect a wrong number, so these four transcribed constants are spelled out once.
    #[test]
    fn the_ladder_matches_the_three_combat_maps() {
        assert_eq!(input_map_for_combat_mode(2).map(|m| m.0), Some(0x1000_0003));
        assert_eq!(input_map_for_combat_mode(4).map(|m| m.0), Some(0x1000_0004));
        assert_eq!(input_map_for_combat_mode(8).map(|m| m.0), Some(0x1000_0005));
        assert_eq!(COMBAT_MAP.0, 0x1000_0002);
        // Peace and undefined fall off both ladders: past the unregister, and out after the
        // register.
        assert_eq!(input_map_for_combat_mode(mode::NONCOMBAT), None);
        assert_eq!(input_map_for_combat_mode(mode::UNDEF), None);
        // Nothing else in the enum is a fighting mode: `COMBAT_COMBAT_MODE` (14) and
        // `VALID_COMBAT_MODES` (15) are masks and never reach `SetCombatMode`'s assignment.
        assert_eq!(input_map_for_combat_mode(14), None);
        assert_eq!(input_map_for_combat_mode(15), None);
    }
}
