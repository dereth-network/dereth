//! Mapped action identifiers.
/// `CombatToggleCombat`.
pub const COMBAT_TOGGLE_COMBAT: u32 = 0x1000_005A;
/// `CombatLowAttack`.
pub const COMBAT_LOW_ATTACK: u32 = 0x1000_005D;
/// `CombatMediumAttack`.
pub const COMBAT_MEDIUM_ATTACK: u32 = 0x1000_005E;
/// `CombatHighAttack`.
pub const COMBAT_HIGH_ATTACK: u32 = 0x1000_005F;

// ---- the rest of the combat-action handler's press and release arms ---------
//
// The ids are the action name table's own, pinned here as literals because
// reading a constant back through the same symbol that wrote it is unfalsifiable.
// A test checks them
// against the shipped `ActionMap` rather than against this list.
//
// **The gauge keys and the height keys are one set of physical keys per mode**: the shipped
// defaults bind `DIK_INSERT`/`DIK_PRIOR`/`DIK_DELETE`/`DIK_END`/`DIK_NEXT` in `MeleeCombat`,
// in `MissileCombat` and in `MagicCombat`, and the input-map registration keeps exactly
// one of the three registered. That is why there is one handler and not three.

/// `CombatDecreaseAttackPower` — `DIK_INSERT` in `MeleeCombat`.
pub const COMBAT_DECREASE_ATTACK_POWER: u32 = 0x1000_005B;
/// `CombatIncreaseAttackPower` — `DIK_PRIOR` in `MeleeCombat`.
pub const COMBAT_INCREASE_ATTACK_POWER: u32 = 0x1000_005C;
/// `CombatDecreaseMissileAccuracy` — `DIK_INSERT` in `MissileCombat`, and it shares
/// the action-handler case with [`COMBAT_DECREASE_ATTACK_POWER`].
pub const COMBAT_DECREASE_MISSILE_ACCURACY: u32 = 0x1000_00EF;
/// `CombatIncreaseMissileAccuracy` — `DIK_PRIOR` in `MissileCombat`.
pub const COMBAT_INCREASE_MISSILE_ACCURACY: u32 = 0x1000_00F0;
/// `CombatAimLow` — `DIK_DELETE` in `MissileCombat`, the low attack height.
pub const COMBAT_AIM_LOW: u32 = 0x1000_00F1;
/// `CombatAimMedium` — `DIK_END` in `MissileCombat`, the medium attack height.
pub const COMBAT_AIM_MEDIUM: u32 = 0x1000_00F2;
/// `CombatAimHigh` — `DIK_NEXT` in `MissileCombat`, the high attack height.
pub const COMBAT_AIM_HIGH: u32 = 0x1000_00F3;

// ---- `handle_magic_action`'s eighteen ---------------------------------

/// `CombatCastCurrentSpell` — `DIK_END` in `MagicCombat`.
pub const COMBAT_CAST_CURRENT_SPELL: u32 = 0x1000_0060;
/// `CombatPrevSpell` — `DIK_DELETE`.
pub const COMBAT_PREV_SPELL: u32 = 0x1000_0061;
/// `CombatNextSpell` — `DIK_NEXT`.
pub const COMBAT_NEXT_SPELL: u32 = 0x1000_0062;
/// `CombatPrevSpellTab` — `DIK_INSERT`.
pub const COMBAT_PREV_SPELL_TAB: u32 = 0x1000_0063;
/// `CombatNextSpellTab` — `DIK_PRIOR`.
pub const COMBAT_NEXT_SPELL_TAB: u32 = 0x1000_0064;
/// `UseSpellSlot_1` — `DIK_1`, and **slot 0**: the client's slot is `action - 0x10000065`.
pub const USE_SPELL_SLOT_FIRST: u32 = 0x1000_0065;
/// The last case of the twelve-entry spell quickslot run. The shipped `MagicCombat` map binds
/// only through `UseSpellSlot_9` (`0x1000006D`); the remaining three are still bindable.
pub const USE_SPELL_SLOT_LAST: u32 = 0x1000_0070;
/// `CombatFirstSpell` — `DIK_LCONTROL+DIK_RCONTROL+DIK_DELETE`.
pub const COMBAT_FIRST_SPELL: u32 = 0x1000_0102;
/// `CombatLastSpell`.
pub const COMBAT_LAST_SPELL: u32 = 0x1000_0103;
/// `CombatFirstSpellTab`.
pub const COMBAT_FIRST_SPELL_TAB: u32 = 0x1000_0104;
/// `CombatLastSpellTab`.
pub const COMBAT_LAST_SPELL_TAB: u32 = 0x1000_0105;
/// `SelectionExamine`.
pub const SELECTION_EXAMINE: u32 = 0x1000_002B;
/// `USE`.
pub const USE: u32 = 0x1000_0025;
/// `SelectionPickUp` — the shipped default key map's `DIK_F`.
///
/// The player-action handler's case 1: with a selected object it places that object in the
/// backpack and consumes the action; with no selection the action is **not** consumed.
///
/// Its next neighbour is `SelectionSplitStack` (case 2), transcribed below;
/// `SelectionPreviousSelection` (case 3) remains unconsumed here.
pub const SELECTION_PICK_UP: u32 = 0x1000_002C;
/// `SelectionSplitStack` — `DIK_T` in `ItemSelectionCommands`. Case 2 sends the selected id
/// to the split-stack receiver and consumes even the zero-id no-op.
pub const SELECTION_SPLIT_STACK: u32 = 0x1000_002D;

// ---- six UI-system action arms -----------------------------
//
// Five actions are selected through a 117-entry byte table indexed by `action - 7`.
// Table entries 0 through 3 select the click, Escape, screenshot and help handlers; entry 4
// is the `return false` fallback used by the other 112 indices. `0x7C` has its own branch
// before the table, while the three high action ids use a subtraction chain after it.

/// `SelectLeft` — **the left mouse button**, not a keyboard select: `crate::actions::ui`
/// names the same id `PRIMARY_CLICK` and `fire::KEYSTONE_SUPPRESSED_ACTIONS` lists 7, 8, 10 and
/// 11 as the click family. Both click actions share the same arm.
pub const SELECT_LEFT: u32 = 0x0000_0007;
/// `SelectRight` — the right mouse button; it shares an arm with [`SELECT_LEFT`].
pub const SELECT_RIGHT: u32 = 0x0000_0008;
/// `EscapeKey` — the longest of the eight arms.
pub const ESCAPE_KEY: u32 = 0x0000_0027;
/// `CaptureScreenshot` — the screenshot arm.
pub const CAPTURE_SCREENSHOT: u32 = 0x0000_0055;

/// The four actions of input map `0x10`, the **system-key swallow**.
///
/// UI initialization is the only site that registers map `0x10`. It installs the client's
/// input-action callback at priority -1, one below the normal lowest priority. That callback
/// returns true without performing any other work.
///
/// So retail's arm for all four *is* the registration: consume the action, do nothing, and —
/// because the callback answered `TRUE` — deny it to the input-handler chain behind
/// the action broadcast, which is where the element manager's
/// global-message-1 broadcast of the action and the visibility toggle live. At priority −1 the
/// map is last in the walk, so any map that binds the same control still wins; this is the
/// floor, not a barrier.
///
/// **What actually happens to each of the four keys is decided in the window procedure,
/// not here**, and this build already reproduces that half in
/// the device input's system-key rule — the system-keys-enabled flag is always false, so a
/// `WM_SYSKEY*` is swallowed after being forwarded to the input manager *except* for two:
///
/// * **Alt+Tab** — swallowed; the client's window never passes it to `DefWindowProc`.
/// * **Alt+Enter** — **not** swallowed: it sets the toggle-full-screen flag, and
///   the event loop's epilogue performs the flip. So yes, Alt+Enter *is*
///   retail's full-screen toggle — through the message pump, never through this action.
/// * **Alt+F4** — **not** swallowed: it falls through to `DefWindowProc`, which closes the
///   window.
/// * **Ctrl+Shift+Esc** — not a `WM_SYSKEY*` at all (no Alt), so the rule above never sees it;
///   the shell takes it before the window does.
///
/// The arm below is therefore the *action* half only, and it is deliberately empty.
pub const SYSTEM_ALT_TAB: u32 = 0x0000_0053;
/// See [`SYSTEM_ALT_TAB`]. `WM_SYSKEYDOWN VK_RETURN` is retail's full-screen toggle, in the
/// pump; the action itself does nothing.
pub const SYSTEM_ALT_ENTER: u32 = 0x0000_007D;
/// See [`SYSTEM_ALT_TAB`]. `WM_SYSKEYDOWN VK_F4` reaches `DefWindowProc` and closes the window;
/// the action itself does nothing.
pub const SYSTEM_ALT_F4: u32 = 0x0000_007E;
/// See [`SYSTEM_ALT_TAB`].
pub const SYSTEM_CTRL_SHIFT_ESC: u32 = 0x0000_007F;
/// `ToggleHelp` — opens help with arguments `(0, 0x10000001)`.
pub const TOGGLE_HELP: u32 = 0x0000_007B;
/// `TogglePluginManager` — the one arm reached by its own branch rather than by the
/// shared dispatch or the subtraction chain.
pub const TOGGLE_PLUGIN_MANAGER: u32 = 0x0000_007C;
/// `ToggleRadarPanel` — the radar-panel visibility action.
pub const TOGGLE_RADAR_PANEL: u32 = 0x1000_001E;
/// `ToggleGameplayOptionsPanel`, which is not an arm of the UI action handler but the argument
/// `EscapeKey`'s "nothing is selected" leg hands on. This is why Escape opens the options
/// panel.
pub const TOGGLE_GAMEPLAY_OPTIONS_PANEL: u32 = 0x1000_001B;

// ---- the sixteen tab-target actions -------------------------------------------
//
// Every one is already in `crate::actions::names`; the names below are that table's, verbatim.
// The player-action handler numbers its cases from `0x1000002A`, and the case number each
// id lands on is given so the arm can be checked against retail's case order.

/// `SelectionSelf` — `case 0`.
pub const SELECTION_SELF: u32 = 0x1000_002A;
/// `SelectionGive`.
pub const SELECTION_GIVE: u32 = 0x1000_0040;
/// `SelectionDrop`.
pub const SELECTION_DROP: u32 = 0x1000_0041;
/// `SelectionMoveToMainPack`.
pub const SELECTION_MOVE_TO_MAIN_PACK: u32 = 0x1000_011C;

/// `SelectionLastAttacker` — `case 0xD`, and the **only** action that reaches
/// the last-attacker range check rather than `select_next`.
pub const SELECTION_LAST_ATTACKER: u32 = 0x1000_0038;

/// `SelectionClosestCompassItem` — `case 4`, `select_next(true, true, COMPASS_ITEM, false)`.
pub const SELECTION_CLOSEST_COMPASS_ITEM: u32 = 0x1000_002F;
/// `SelectionPreviousCompassItem` — `case 5`.
pub const SELECTION_PREVIOUS_COMPASS_ITEM: u32 = 0x1000_0030;
/// `SelectionNextCompassItem` — `case 6`.
pub const SELECTION_NEXT_COMPASS_ITEM: u32 = 0x1000_0031;
/// `SelectionClosestItem` — `case 7`, and the **only** one of the twenty-six call sites that
/// passes `exclude_own_wielded = true`.
///
/// **And it is inert there.**
/// The select-next item-selection arm rejects any object with a non-zero wielder
/// outright — *any* wielder — while the exclude-own-wielded gate rejects only an object the
/// player wields, a strict subset of it for every non-zero player id. So
/// "closest item" and "next item" are the **same** about a wielded object: neither can pick
/// one. The widely-repeated consequence — *"'closest item' will not pick your own drawn weapon
/// while 'next item' will"* — is false, and a test asserts it both ways.
pub const SELECTION_CLOSEST_ITEM: u32 = 0x1000_0032;
/// `SelectionPreviousItem` — `case 8`.
pub const SELECTION_PREVIOUS_ITEM: u32 = 0x1000_0033;
/// `SelectionNextItem` — `case 9`.
pub const SELECTION_NEXT_ITEM: u32 = 0x1000_0034;
/// `SelectionClosestMonster` — `case 10`.
pub const SELECTION_CLOSEST_MONSTER: u32 = 0x1000_0035;
/// `SelectionPreviousMonster` — `case 0xB`.
pub const SELECTION_PREVIOUS_MONSTER: u32 = 0x1000_0036;
/// `SelectionNextMonster` — `case 0xC`.
pub const SELECTION_NEXT_MONSTER: u32 = 0x1000_0037;
/// `SelectionClosestPlayer` — `case 0xE`.
pub const SELECTION_CLOSEST_PLAYER: u32 = 0x1000_0039;
/// `SelectionPreviousPlayer` — `case 0xF`.
pub const SELECTION_PREVIOUS_PLAYER: u32 = 0x1000_003A;
/// `SelectionNextPlayer` — `case 0x10`.
pub const SELECTION_NEXT_PLAYER: u32 = 0x1000_003B;
/// `SelectionPreviousSelection`, the shipped `DIK_P` —
/// case `0x1000002E`.
pub const SELECTION_PREVIOUS_SELECTION: u32 = 0x1000_002E;
/// `SelectionPreviousFellow`, the shipped `DIK_N` —
/// fellowship selection's previous-member operation.
pub const SELECTION_PREVIOUS_FELLOW: u32 = 0x1000_003C;
/// `SelectionNextFellow`, the shipped `DIK_M` —
/// fellowship selection's next-member operation.
pub const SELECTION_NEXT_FELLOW: u32 = 0x1000_003D;
/// `SelectionUseClosestUnopenedCorpse` — `case 0x13`, which selects **and then uses**.
pub const SELECTION_USE_CLOSEST_UNOPENED_CORPSE: u32 = 0x1000_003E;
/// `SelectionUseNextUnopenedCorpse` — `case 0x14`, which selects, wraps, **and then uses**.
pub const SELECTION_USE_NEXT_UNOPENED_CORPSE: u32 = 0x1000_003F;
/// `SelectionClosestUnopenedCorpse` — `case 0x43`: the same selection as `case 0x13` with
/// **no** object-use call.
pub const SELECTION_CLOSEST_UNOPENED_CORPSE: u32 = 0x1000_0121;
/// `SelectionNextUnopenedCorpse` — `case 0x44`: the same as `case 0x14` with no object use.
pub const SELECTION_NEXT_UNOPENED_CORPSE: u32 = 0x1000_0122;
