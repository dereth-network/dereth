//! The action vocabulary: what a player, a script or a second front end asks the client to do.
//!
//! The client's seam for control is an **action**, never a device event. A keyboard, a gamepad,
//! a test script and a bot all end in the same place: an `ActionId` from the retail action list
//! with an `ActionPhase` (it began, it repeated, it ended) and an extent. How a device becomes an
//! action -- keys, key maps, modifiers, the toggle and repeat machine, window messages -- belongs
//! to the front end that owns the device; nothing here names a key.
//!
//! The ids are the shipped `ActionMap`'s, and `names` is the enum table the client writes them
//! out with (the names a `.keymap` file uses). The id groups the UI and the handlers refer to by
//! name are `ui`, `movement`, [`camera`] and `chat_entry`.
//!
//! This is the one crate both sides of the seam may name: the runtime consumes actions, and a
//! front end's device pipeline, which may not depend on the runtime, produces them.

/// A numeric action id. The values are the shipped `ActionMap`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ActionId(pub u32);

/// Where in its life an action is.
///
/// A held control produces [`ActionPhase::Begin`] when it goes down, [`ActionPhase::Repeat`] while
/// its toggle type repeats, and [`ActionPhase::End`] when it comes up. A one-shot produces a single
/// `Begin`. A second control joining a held action, or leaving it while another still holds it,
/// produces another `Begin` carrying the new extent.
///
/// Every handler that asks "did this start?" treats `Repeat` as a start: the client's handlers
/// test one start flag, and a repeat carries it set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionPhase {
    /// The action began, or a held action's extent changed.
    Begin,
    /// A held action repeated. [`Action::repeats`] says how many repeats this one stands for:
    /// more than one after a stalled frame, because repeat is catch-up.
    Repeat,
    /// The action ended.
    End,
}

/// One action, as the runtime's handlers receive it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Action {
    /// Which action.
    pub id: ActionId,
    /// Begin, repeat or end.
    pub phase: ActionPhase,
    /// How far: 1.0 for a key, the deflection for an axis, 0.0 on the end.
    pub extent: f32,
    /// For [`ActionPhase::Repeat`], how many repeats this dispatch stands for; otherwise zero.
    pub repeats: u32,
}

impl Action {
    /// The action beginning at full extent: a key going down, or a one-shot.
    #[must_use]
    pub const fn begin(id: ActionId) -> Self {
        Self {
            id,
            phase: ActionPhase::Begin,
            extent: 1.0,
            repeats: 0,
        }
    }

    /// The action ending: a key coming up.
    #[must_use]
    pub const fn end(id: ActionId) -> Self {
        Self {
            id,
            phase: ActionPhase::End,
            extent: 0.0,
            repeats: 0,
        }
    }

    /// `repeats` repeats of a held action at full extent.
    #[must_use]
    pub const fn repeat(id: ActionId, repeats: u32) -> Self {
        Self {
            id,
            phase: ActionPhase::Repeat,
            extent: 1.0,
            repeats,
        }
    }

    /// The same action with another extent.
    #[must_use]
    pub const fn with_extent(mut self, extent: f32) -> Self {
        self.extent = extent;
        self
    }

    /// Whether this is a start: a begin or a repeat. This is the one start flag the client's
    /// handlers test.
    #[must_use]
    pub const fn is_start(&self) -> bool {
        !matches!(self.phase, ActionPhase::End)
    }
}

/// The action ids the UI itself cares about: the pointer, the taps, the wheel and text editing.
///
/// UI input handling arms a drag on [`ui::PRIMARY_CLICK`] and starts it once the pointer has moved
/// [`ui::DRAG_THRESHOLD_SQUARED`], so the threshold lives with the ids rather than with the
/// element tree.
pub mod ui {
    /// Primary click — takes mouse capture and arms drag-and-drop.
    pub const PRIMARY_CLICK: u32 = 7;
    /// Secondary click — raises element message 0x27 on elements with a context menu enabled.
    pub const SECONDARY_CLICK: u32 = 8;
    /// Middle click.
    pub const MIDDLE_CLICK: u32 = 9;
    /// The three "tap" actions: they do **not** take mouse capture and go straight to
    /// mouse-tap notification.
    pub const TAPS: [u32; 3] = [0x0D, 0x0E, 0x0F];
    /// The wheel, `DIMOFS_Z[+]` / `DIMOFS_Z[-]` in input map `0xA`.
    pub const WHEEL_UP: u32 = 5;
    /// See [`WHEEL_UP`].
    pub const WHEEL_DOWN: u32 = 6;
    /// The action the client consumes and does nothing with, on both
    /// edges — its first `case 4:`.
    pub const IGNORED: u32 = 4;
    /// The lowest and highest action the press handler forwards to mouse-down dispatch:
    /// forward `(action, extent)` exactly when `4 < action && action < 0x10`.
    pub const MOUSE_ACTIONS: std::ops::RangeInclusive<u32> = 5..=0x0F;
    /// The narrower range its release arm forwards to mouse-up dispatch: `case 5..0xC`. 0x0D–0x0F are
    /// the taps, which are `return true` on a release and raise nothing.
    pub const MOUSE_UP_ACTIONS: std::ops::RangeInclusive<u32> = 5..=0x0C;

    // Text editing.
    pub const CURSOR_LEFT: u32 = 0x16;
    pub const CURSOR_RIGHT: u32 = 0x17;
    pub const CURSOR_WORD_LEFT: u32 = 0x18;
    pub const CURSOR_WORD_RIGHT: u32 = 0x19;
    pub const CURSOR_LINE_HOME: u32 = 0x1A;
    pub const CURSOR_LINE_END: u32 = 0x1B;
    pub const CURSOR_HOME: u32 = 0x1C;
    pub const CURSOR_END: u32 = 0x1D;
    pub const CURSOR_UP: u32 = 0x1E;
    pub const CURSOR_DOWN: u32 = 0x1F;
    pub const CURSOR_PAGE_UP: u32 = 0x20;
    pub const CURSOR_PAGE_DOWN: u32 = 0x21;
    pub const COPY: u32 = 0x22;
    pub const CUT: u32 = 0x23;
    pub const PASTE: u32 = 0x24;
    pub const ACCEPT: u32 = 0x25;
    pub const DELETE: u32 = 0x26;
    pub const ESCAPE: u32 = 0x27;
    pub const BACKSPACE: u32 = 0x28;

    #[must_use]
    pub const fn is_tap(a: u32) -> bool {
        a == TAPS[0] || a == TAPS[1] || a == TAPS[2]
    }

    /// The drag threshold requires `dx² + dy² > 15` -- about four pixels.
    ///
    /// UI input handling arms a drag on [`PRIMARY_CLICK`] and starts it once the pointer has moved
    /// this far, so the threshold belongs with the action ids rather than with the element tree.
    pub const DRAG_THRESHOLD_SQUARED: i32 = 15;
}

/// The movement actions: walking, turning, strafing, the run lock, jump and the four stances.
pub mod movement {
    use super::ActionId;

    pub const MOVE_FORWARD: ActionId = ActionId(0x29);
    pub const MOVE_BACKWARD: ActionId = ActionId(0x2A);
    pub const STOP_MOVING: ActionId = ActionId(0x2B);
    pub const STRAFE_RIGHT: ActionId = ActionId(0x2C);
    pub const STRAFE_LEFT: ActionId = ActionId(0x2D);
    pub const TURN_RIGHT: ActionId = ActionId(0x2E);
    pub const TURN_LEFT: ActionId = ActionId(0x2F);
    pub const AUTORUN: ActionId = ActionId(0x30);
    pub const JUMP: ActionId = ActionId(0x31);
    pub const TOGGLE_RUN_WALK: ActionId = ActionId(0x32);
    /// The four stance actions the ready action also carries.
    pub const READY: ActionId = ActionId(0x1000_0094);
    pub const CROUCH: ActionId = ActionId(0x1000_0095);
    pub const SIT: ActionId = ActionId(0x1000_0096);
    pub const LAY_DOWN: ActionId = ActionId(0x1000_0097);
}

/// The camera actions: zoom, rotation, the view modes and the mouse-look toggles.
pub mod camera {
    use super::ActionId;

    pub const ZOOM_IN: ActionId = ActionId(0x33);
    pub const ZOOM_OUT: ActionId = ActionId(0x34);
    pub const ROTATE_LEFT: ActionId = ActionId(0x35);
    pub const ROTATE_RIGHT: ActionId = ActionId(0x36);
    pub const ROTATE_UP: ActionId = ActionId(0x37);
    pub const ROTATE_DOWN: ActionId = ActionId(0x38);
    pub const MOVE_TO_DEFAULT: ActionId = ActionId(0x39);
    pub const FIRST_PERSON: ActionId = ActionId(0x3A);
    pub const OVERHEAD_VIEW: ActionId = ActionId(0x3B);
    pub const MAP_VIEW: ActionId = ActionId(0x3C);
    pub const TOGGLE_MOUSELOOK: ActionId = ActionId(0x3D);
    pub const TOGGLE_ALTERNATE_MODE: ActionId = ActionId(0x3E);
}

/// The chat entry's actions: begin and toggle chat mode, start a command, and the reply keys
/// that pre-fill the line.
pub mod chat_entry {
    use super::ActionId;

    /// *Begin Chat Mode*, default `DIK_RETURN`.
    pub const BEGIN_CHAT_MODE: ActionId = ActionId(0x1000_0023);
    /// *Enter/Exit Chat Mode*, default `DIK_TAB`. In its own input map (`0x1000000D`) at priority
    /// 3010 so it still works while the chat bar has focus.
    pub const TOGGLE_CHAT_ENTRY: ActionId = ActionId(0x1000_0024);
    /// *Start Command* — pre-fills the field with the `/` prefix from string-table enum 6.
    pub const START_COMMAND: ActionId = ActionId(0x1000_0028);
    pub const REPLY: ActionId = ActionId(0x1000_0022);
    pub const MONARCH_REPLY: ActionId = ActionId(0x1000_0020);
    pub const PATRON_REPLY: ActionId = ActionId(0x1000_0021);
    pub const TELL_TO_SELECTED: ActionId = ActionId(0x1000_0119);
}

/// This client's own actions, beside the retail ones: what any interface can bind and the
/// retail action table has no row for. Their ids are above every retail id, and their names are
/// in [`names::DERETH_ACTION_NAMES`], so a key map file names them as it names a retail action.
pub mod dereth {
    /// Show or hide the performance panel (frame rate and frame time), whatever the interface.
    pub const TOGGLE_PERFORMANCE_PANEL: u32 = 0x2000_0001;
    /// Hold to step sideways with the turning keys (the classic interface's Hold Sidestep).
    pub const MOVEMENT_HOLD_SIDESTEP: u32 = 0x2000_0002;
    /// Show or hide the secure-trade window (the classic interface's Trade panel key).
    pub const TOGGLE_TRADE_PANEL: u32 = 0x2000_0003;
    /// Show or hide the spell research window, on a world with spell research.
    pub const TOGGLE_SPELL_RESEARCH_PANEL: u32 = 0x2000_0004;
    /// Flip the character option that makes a shortcut of an item used.
    pub const PLAYER_OPTION_AUTO_CREATE_SHORTCUTS: u32 = 0x2000_0005;
    /// Flip the classic interface's inverted vertical mouse look.
    pub const TOGGLE_INVERT_MOUSE_LOOK: u32 = 0x2000_0006;
    /// Flip the classic interface's right-click mouse look.
    pub const TOGGLE_RIGHT_CLICK_MOUSE_LOOK: u32 = 0x2000_0007;
    /// Flip the classic interface's stretched layout.
    pub const TOGGLE_STRETCH_UI: u32 = 0x2000_0008;
    /// Flip whether sounds play only while the game's window is active.
    pub const TOGGLE_MUTE_ON_LOSING_FOCUS: u32 = 0x2000_0009;
}

/// The action names: the enum table the client writes an action out with (field `0x19` of a
/// `.keymap` file) and reads one back by. Read out of the retail portal dat; the combat and magic
/// actions have human-readable names there too.
pub mod names {
    use super::ActionId;

    /// Every action id the shipped enum mapper names, in id order.
    pub const ACTION_ENUM_NAMES: &[(u32, &str)] = &[
        (0x00000000, "Invalid"),
        (0x00000001, "DoNothing"),
        (0x00000002, "PointerX"),
        (0x00000003, "PointerY"),
        (0x00000004, "Scroll"),
        (0x00000005, "ScrollUp"),
        (0x00000006, "ScrollDown"),
        (0x00000007, "SelectLeft"),
        (0x00000008, "SelectRight"),
        (0x00000009, "SelectMid"),
        (0x0000000A, "SelectDblLeft"),
        (0x0000000B, "SelectDblRight"),
        (0x0000000C, "SelectDblMid"),
        (0x0000000D, "TapLeft"),
        (0x0000000E, "TapRight"),
        (0x0000000F, "TapMid"),
        (0x00000010, "FocusNext"),
        (0x00000011, "FocusPrevious"),
        (0x00000012, "FocusMoveLeft"),
        (0x00000013, "FocusMoveRight"),
        (0x00000014, "FocusMoveUp"),
        (0x00000015, "FocusMoveDown"),
        (0x00000016, "CursorCharLeft"),
        (0x00000017, "CursorCharRight"),
        (0x00000018, "CursorWordLeft"),
        (0x00000019, "CursorWordRight"),
        (0x0000001A, "CursorStartOfLine"),
        (0x0000001B, "CursorEndOfLine"),
        (0x0000001C, "CursorStartOfDocument"),
        (0x0000001D, "CursorEndOfDocument"),
        (0x0000001E, "CursorPreviousLine"),
        (0x0000001F, "CursorNextLine"),
        (0x00000020, "CursorPreviousPage"),
        (0x00000021, "CursorNextPage"),
        (0x00000022, "CopyText"),
        (0x00000023, "CutText"),
        (0x00000024, "PasteText"),
        (0x00000025, "AcceptInput"),
        (0x00000026, "DeleteKey"),
        (0x00000027, "EscapeKey"),
        (0x00000028, "BackspaceKey"),
        (0x00000029, "MovementForward"),
        (0x0000002A, "MovementBackup"),
        (0x0000002B, "MovementStop"),
        (0x0000002C, "MovementStrafeRight"),
        (0x0000002D, "MovementStrafeLeft"),
        (0x0000002E, "MovementTurnRight"),
        (0x0000002F, "MovementTurnLeft"),
        (0x00000030, "MovementRunLock"),
        (0x00000031, "MovementJump"),
        (0x00000032, "MovementWalkMode"),
        (0x00000033, "CameraMoveToward"),
        (0x00000034, "CameraMoveAway"),
        (0x00000035, "CameraRotateLeft"),
        (0x00000036, "CameraRotateRight"),
        (0x00000037, "CameraRotateUp"),
        (0x00000038, "CameraRotateDown"),
        (0x00000039, "CameraViewDefault"),
        (0x0000003A, "CameraViewFirstPerson"),
        (0x0000003B, "CameraViewLookDown"),
        (0x0000003C, "CameraViewMapMode"),
        (0x0000003D, "CameraInstantMouseLook"),
        (0x0000003E, "CameraActivateAlternateMode"),
        (0x0000003F, "Internal_CameraAdjustRight"),
        (0x00000040, "Internal_CameraAdjustLeft"),
        (0x00000041, "Internal_CameraAdjustForward"),
        (0x00000042, "Internal_CameraAdjustBackward"),
        (0x00000043, "Internal_CameraAdjustUp"),
        (0x00000044, "Internal_CameraAdjustDown"),
        (0x00000045, "Internal_CameraAdjustPivotRight"),
        (0x00000046, "Internal_CameraAdjustPivotLeft"),
        (0x00000047, "Internal_CameraAdjustPivotUp"),
        (0x00000048, "Internal_CameraAdjustPivotDown"),
        (0x00000049, "Internal_CameraAdjustPivotForward"),
        (0x0000004A, "Internal_CameraAdjustPivotBackward"),
        (0x0000004B, "Internal_CameraIncreaseAdjustSpeed"),
        (0x0000004C, "Internal_CameraDecreaseAdjustSpeed"),
        (0x0000004D, "Internal_CameraIncreaseStiffness"),
        (0x0000004E, "Internal_CameraDecreaseStiffness"),
        (0x0000004F, "Internal_CameraLookInDirection"),
        (0x00000050, "Internal_CameraLookAtObject"),
        (0x00000051, "Internal_CameraLookAtPivot"),
        (0x00000052, "Internal_CameraAlignToPlane"),
        (0x00000053, "AltTab"),
        (0x00000054, "UI_TOGGLE"),
        (0x00000055, "CaptureScreenshot"),
        (0x00000056, "ToggleDebugConsole"),
        (0x00000057, "ToggleProfiler"),
        (0x00000058, "ToggleBenchmarkHUD"),
        (0x00000059, "ToggleDebugHUD"),
        (0x0000005A, "ShowUIDebuggingWindow"),
        (0x0000005B, "PreprocExit"),
        (0x0000005C, "PreprocSkip"),
        (0x0000005D, "PreprocMoreDetail"),
        (0x0000005E, "PreprocLessDetail"),
        (0x0000005F, "PreprocDrawNormals"),
        (0x00000060, "PreprocDrawSolid"),
        (0x00000061, "PreprocDrawGrid"),
        (0x00000062, "PreprocDrawHighlight"),
        (0x00000063, "PreprocSuperLongJump"),
        (0x00000064, "PreprocSuperHighJump"),
        (0x00000065, "PreprocJump"),
        (0x00000066, "PreprocStopMotion"),
        (0x00000067, "PreprocIncreaseSpeed"),
        (0x00000068, "PreprocDecreaseSpeed"),
        (0x00000069, "PreprocFlyForward"),
        (0x0000006A, "PreprocFlyDown"),
        (0x0000006B, "PreprocFlyLeft"),
        (0x0000006C, "PreprocFlyRight"),
        (0x0000006D, "PreprocIncreaseViewDistance"),
        (0x0000006E, "PreprocDecreaseViewDistance"),
        (0x0000006F, "PreprocTestResize"),
        (0x00000070, "PreprocToggleDetailTextures"),
        (0x00000071, "PreprocTeleport"),
        (0x00000072, "PreprocToggleClothing"),
        (0x00000073, "PreprocToggleCellHighlight"),
        (0x00000074, "PreprocRenderRadius"),
        (0x00000075, "GFXIncreaseGamma"),
        (0x00000076, "GFXDecreaseGamma"),
        (0x00000077, "GFXPerfMoreSpeed"),
        (0x00000078, "GFXPerfMoreDetail"),
        (0x00000079, "GFXPerfToggleAutoLOD"),
        (0x0000007A, "PreprocCycleTextureResolution"),
        (0x0000007B, "ToggleHelp"),
        (0x0000007C, "TogglePluginManager"),
        (0x0000007D, "AltEnter"),
        (0x0000007E, "AltF4"),
        (0x0000007F, "CtrlShiftEsc"),
        (0x10000001, "ToggleCasPanel"),
        (0x10000002, "ToggleAdminPanel"),
        (0x10000003, "ToggleAbusePanel"),
        (0x10000004, "ToggleBookPanel"),
        (0x10000005, "ToggleCharacterInfoPanel"),
        (0x10000006, "TogglePositiveEffectsPanel"),
        (0x10000007, "ToggleNegativeEffectsPanel"),
        (0x10000008, "ToggleExaminationPanel"),
        (0x10000009, "ToggleLinkStatusPanel"),
        (0x1000000A, "ToggleMiniGamePanel"),
        (0x1000000B, "ToggleUrgentAssistancePanel"),
        (0x1000000C, "ToggleVitaePanel"),
        (0x1000000D, "ToggleSocialPanel"),
        (0x1000000E, "ToggleAllegiancePanel"),
        (0x1000000F, "ToggleFellowshipPanel"),
        (0x10000010, "ToggleSpellManagementPanel"),
        (0x10000011, "ToggleSpellbookPanel"),
        (0x10000012, "ToggleSpellComponentsPanel"),
        (0x10000013, "ToggleSkillManagementPanel"),
        (0x10000014, "ToggleAttributesPanel"),
        (0x10000015, "ToggleSkillsPanel"),
        (0x10000016, "ToggleWorldPanel"),
        (0x10000017, "ToggleMapPanel"),
        (0x10000018, "ToggleHousePanel"),
        (0x10000019, "ToggleInventoryPanel"),
        (0x1000001A, "ToggleOptionsPanel"),
        (0x1000001B, "ToggleGameplayOptionsPanel"),
        (0x1000001C, "ToggleCharacterOptionsPanel"),
        (0x1000001D, "ToggleConfigOptionsPanel"),
        (0x1000001E, "ToggleRadarPanel"),
        (0x1000001F, "ToggleKeyboardPanel"),
        (0x10000020, "MonarchReply"),
        (0x10000021, "PatronReply"),
        (0x10000022, "Reply"),
        (0x10000023, "EnterChatMode"),
        (0x10000024, "ToggleChatEntry"),
        (0x10000025, "USE"),
        (0x10000026, "LOGOUT"),
        (0x10000027, "EXITGAME"),
        (0x10000028, "START_COMMAND"),
        (0x10000029, "START_ALIAS"),
        (0x1000002A, "SelectionSelf"),
        (0x1000002B, "SelectionExamine"),
        (0x1000002C, "SelectionPickUp"),
        (0x1000002D, "SelectionSplitStack"),
        (0x1000002E, "SelectionPreviousSelection"),
        (0x1000002F, "SelectionClosestCompassItem"),
        (0x10000030, "SelectionPreviousCompassItem"),
        (0x10000031, "SelectionNextCompassItem"),
        (0x10000032, "SelectionClosestItem"),
        (0x10000033, "SelectionPreviousItem"),
        (0x10000034, "SelectionNextItem"),
        (0x10000035, "SelectionClosestMonster"),
        (0x10000036, "SelectionPreviousMonster"),
        (0x10000037, "SelectionNextMonster"),
        (0x10000038, "SelectionLastAttacker"),
        (0x10000039, "SelectionClosestPlayer"),
        (0x1000003A, "SelectionPreviousPlayer"),
        (0x1000003B, "SelectionNextPlayer"),
        (0x1000003C, "SelectionPreviousFellow"),
        (0x1000003D, "SelectionNextFellow"),
        (0x1000003E, "SelectionUseClosestUnopenedCorpse"),
        (0x1000003F, "SelectionUseNextUnopenedCorpse"),
        (0x10000040, "SelectionGive"),
        (0x10000041, "SelectionDrop"),
        (0x10000042, "UseQuickSlot_1"),
        (0x10000043, "UseQuickSlot_2"),
        (0x10000044, "UseQuickSlot_3"),
        (0x10000045, "UseQuickSlot_4"),
        (0x10000046, "UseQuickSlot_5"),
        (0x10000047, "UseQuickSlot_6"),
        (0x10000048, "UseQuickSlot_7"),
        (0x10000049, "UseQuickSlot_8"),
        (0x1000004A, "UseQuickSlot_9"),
        (0x1000004B, "UseQuickSlot_10"),
        (0x1000004C, "UseQuickSlot_11"),
        (0x1000004D, "UseQuickSlot_12"),
        (0x1000004E, "SelectQuickSlot_1"),
        (0x1000004F, "SelectQuickSlot_2"),
        (0x10000050, "SelectQuickSlot_3"),
        (0x10000051, "SelectQuickSlot_4"),
        (0x10000052, "SelectQuickSlot_5"),
        (0x10000053, "SelectQuickSlot_6"),
        (0x10000054, "SelectQuickSlot_7"),
        (0x10000055, "SelectQuickSlot_8"),
        (0x10000056, "SelectQuickSlot_9"),
        (0x10000057, "SelectQuickSlot_10"),
        (0x10000058, "SelectQuickSlot_11"),
        (0x10000059, "SelectQuickSlot_12"),
        (0x1000005A, "CombatToggleCombat"),
        (0x1000005B, "CombatDecreaseAttackPower"),
        (0x1000005C, "CombatIncreaseAttackPower"),
        (0x1000005D, "CombatLowAttack"),
        (0x1000005E, "CombatMediumAttack"),
        (0x1000005F, "CombatHighAttack"),
        (0x10000060, "CombatCastCurrentSpell"),
        (0x10000061, "CombatPrevSpell"),
        (0x10000062, "CombatNextSpell"),
        (0x10000063, "CombatPrevSpellTab"),
        (0x10000064, "CombatNextSpellTab"),
        (0x10000065, "UseSpellSlot_1"),
        (0x10000066, "UseSpellSlot_2"),
        (0x10000067, "UseSpellSlot_3"),
        (0x10000068, "UseSpellSlot_4"),
        (0x10000069, "UseSpellSlot_5"),
        (0x1000006A, "UseSpellSlot_6"),
        (0x1000006B, "UseSpellSlot_7"),
        (0x1000006C, "UseSpellSlot_8"),
        (0x1000006D, "UseSpellSlot_9"),
        (0x1000006E, "UseSpellSlot_10"),
        (0x1000006F, "UseSpellSlot_11"),
        (0x10000070, "UseSpellSlot_12"),
        (0x10000071, "PlayerOption_AutoRepeatAttack"),
        (0x10000072, "PlayerOption_IgnoreAllegianceRequests"),
        (0x10000073, "PlayerOption_IgnoreFellowshipRequests"),
        (0x10000074, "PlayerOption_IgnoreTradeRequests"),
        (0x10000075, "PlayerOption_DisableMostWeatherEffects"),
        (0x10000076, "PlayerOption_PersistentAtDay"),
        (0x10000077, "PlayerOption_AllowGive"),
        (0x10000078, "PlayerOption_ViewCombatTarget"),
        (0x10000079, "PlayerOption_ShowTooltips"),
        (0x1000007A, "PlayerOption_UseDeception"),
        (0x1000007B, "PlayerOption_ToggleRun"),
        (0x1000007C, "PlayerOption_StayInChatMode"),
        (0x1000007D, "PlayerOption_AdvancedCombatUI"),
        (0x1000007E, "PlayerOption_AutoTarget"),
        (0x1000007F, "PlayerOption_VividTargetingIndicator"),
        (0x10000080, "PlayerOption_FellowshipShareXP"),
        (0x10000081, "PlayerOption_AcceptLootPermits"),
        (0x10000082, "PlayerOption_FellowshipShareLoot"),
        (0x10000083, "PlayerOption_FellowshipAutoAcceptRequests"),
        (0x10000085, "PlayerOption_CoordinatesOnRadar"),
        (0x10000086, "PlayerOption_SpellDuration"),
        (0x10000087, "PlayerOption_DisableHouseRestrictionEffects"),
        (0x10000088, "PlayerOption_DragItemOnPlayerOpensSecureTrade"),
        (
            0x10000089,
            "PlayerOption_DisplayAllegianceLogonNotifications",
        ),
        (0x1000008A, "PlayerOption_UseChargeAttack"),
        (0x1000008B, "PlayerOption_UseCraftSuccessDialog"),
        (0x1000008C, "PlayerOption_HearAllegianceChat"),
        (0x1000008D, "PlayerOption_DisplayDateOfBirth"),
        (0x1000008E, "PlayerOption_DisplayAge"),
        (0x1000008F, "PlayerOption_DisplayChessRank"),
        (0x10000090, "PlayerOption_DisplayFishingSkill"),
        (0x10000091, "PlayerOption_DisplayNumberDeaths"),
        (0x10000092, "PlayerOption_DisplayTimeStamps"),
        (0x10000093, "PlayerOption_SalvageMultiple"),
        (0x10000094, "Ready"),
        (0x10000095, "Crouch"),
        (0x10000096, "Sitting"),
        (0x10000097, "Sleeping"),
        (0x10000098, "AFKState"),
        (0x10000099, "Akimbo"),
        (0x1000009A, "ATOYOT"),
        (0x1000009B, "AkimboState"),
        (0x1000009C, "AtEaseState"),
        (0x1000009D, "Beckon"),
        (0x1000009E, "BeSeeingYou"),
        (0x1000009F, "BlowKiss"),
        (0x100000A0, "BowDeep"),
        (0x100000A1, "BowDeepState"),
        (0x100000A2, "Cheer"),
        (0x100000A3, "ClapHands"),
        (0x100000A4, "ClapHandsState"),
        (0x100000A5, "Cringe"),
        (0x100000A6, "CrossArmsState"),
        (0x100000A7, "Cry"),
        (0x100000A8, "CurtseyState"),
        (0x100000A9, "DrudgeDance"),
        (0x100000AA, "DrudgeDanceState"),
        (0x100000AB, "HaveASeat"),
        (0x100000AC, "HaveASeatState"),
        (0x100000AD, "HeartyLaugh"),
        (0x100000AE, "Helper"),
        (0x100000AF, "Kneel"),
        (0x100000B0, "KneelState"),
        (0x100000B1, "Knock"),
        (0x100000B2, "Laugh"),
        (0x100000B3, "LeanState"),
        (0x100000B4, "MeditateState"),
        (0x100000B5, "MimeDrink"),
        (0x100000B6, "MimeEat"),
        (0x100000B7, "Mock"),
        (0x100000B8, "Nod"),
        (0x100000B9, "NudgeLeft"),
        (0x100000BA, "NudgeRight"),
        (0x100000BB, "Plead"),
        (0x100000BC, "PleadState"),
        (0x100000BD, "Point"),
        (0x100000BE, "PointState"),
        (0x100000BF, "PointDown"),
        (0x100000C0, "PointDownState"),
        (0x100000C1, "PointLeft"),
        (0x100000C2, "PointLeftState"),
        (0x100000C3, "PointRight"),
        (0x100000C4, "PointRightState"),
        (0x100000C5, "PossumState"),
        (0x100000C6, "Pray"),
        (0x100000C7, "PrayState"),
        (0x100000C8, "ReadState"),
        (0x100000C9, "Salute"),
        (0x100000CA, "SaluteState"),
        (0x100000CB, "ScanHorizon"),
        (0x100000CC, "ScratchHead"),
        (0x100000CD, "ScratchHeadState"),
        (0x100000CE, "ShakeFist"),
        (0x100000CF, "ShakeFistState"),
        (0x100000D0, "ShakeHead"),
        (0x100000D1, "Shiver"),
        (0x100000D2, "ShiverState"),
        (0x100000D3, "Shoo"),
        (0x100000D4, "Shrug"),
        (0x100000D5, "SitState"),
        (0x100000D6, "SitBackState"),
        (0x100000D7, "SitCrossleggedState"),
        (0x100000D8, "Slouch"),
        (0x100000D9, "SlouchState"),
        (0x100000DA, "SmackHead"),
        (0x100000DB, "SnowAngelState"),
        (0x100000DC, "Spit"),
        (0x100000DD, "Surrender"),
        (0x100000DE, "SurrenderState"),
        (0x100000DF, "TalktotheHandState"),
        (0x100000E0, "TapFoot"),
        (0x100000E1, "TapFootState"),
        (0x100000E2, "Teapot"),
        (0x100000E3, "ThinkerState"),
        (0x100000E4, "WarmHands"),
        (0x100000E5, "Wave"),
        (0x100000E6, "WaveState"),
        (0x100000E7, "WaveLow"),
        (0x100000E8, "WaveHigh"),
        (0x100000E9, "Winded"),
        (0x100000EA, "WindedState"),
        (0x100000EB, "Woah"),
        (0x100000EC, "WoahState"),
        (0x100000ED, "YawnStretch"),
        (0x100000EE, "YMCA"),
        (0x100000EF, "CombatDecreaseMissileAccuracy"),
        (0x100000F0, "CombatIncreaseMissileAccuracy"),
        (0x100000F1, "CombatAimLow"),
        (0x100000F2, "CombatAimMedium"),
        (0x100000F3, "CombatAimHigh"),
        (0x10000102, "CombatFirstSpell"),
        (0x10000103, "CombatLastSpell"),
        (0x10000104, "CombatFirstSpellTab"),
        (0x10000105, "CombatLastSpellTab"),
        (0x1000010D, "CreateShortcut"),
        (0x1000010E, "PlayerOption_HearGeneralChat"),
        (0x1000010F, "PlayerOption_HearTradeChat"),
        (0x10000110, "PlayerOption_HearLFGChat"),
        (0x10000112, "PlayerOption_HearRoleplayChat"),
        (0x10000113, "ToggleChatOptionsPanel"),
        (0x10000114, "ToggleFloatingChatWindow1"),
        (0x10000115, "ToggleFloatingChatWindow2"),
        (0x10000116, "ToggleFloatingChatWindow3"),
        (0x10000117, "ToggleFloatingChatWindow4"),
        (0x10000118, "ToggleFriendsPanel"),
        (0x10000119, "TellSelected"),
        (0x1000011A, "ToggleCharacterTitlePanel"),
        (0x1000011B, "PlayerOption_DisplayNumberCharacterTitles"),
        (0x1000011C, "SelectionMoveToMainPack"),
        (0x1000011D, "PlayerOption_MainPackPreferred"),
        (0x1000011E, "PlayerOption_LeadMissileTargets"),
        (0x1000011F, "PlayerOption_UseFastMissiles"),
        (0x10000120, "PlayerOption_FilterLanguage"),
        (0x10000121, "SelectionClosestUnopenedCorpse"),
        (0x10000122, "SelectionNextUnopenedCorpse"),
        (0x10000123, "PlayerOption_ConfirmVolatileRareUse"),
        (0x10000124, "ToggleSquelchPanel"),
        (0x10000125, "PlayerOption_HearSocietyChat"),
        (0x10000127, "ToggleQuestManagementPanel"),
        (0x10000128, "ToggleJournalPanel"),
        (0x10000129, "TogglePageListPanel"),
        (0x1000012A, "PlayerOption_ShowHelm"),
        (0x1000012C, "PlayerOption_DisableDistanceFog"),
        (0x1000012D, "PlayerOption_UseMouseTurning"),
        (0x1000012E, "ToggleContractsPanel"),
        (0x1000012F, "PlayerOption_ShowCloak"),
        (0x10000130, "ToggleFloatingExaminationWindow"),
        (0x10000131, "ToggleOptionsMenu"),
        (0x10000132, "UseQuickSlot_13"),
        (0x10000133, "UseQuickSlot_14"),
        (0x10000134, "UseQuickSlot_15"),
        (0x10000135, "UseQuickSlot_16"),
        (0x10000136, "UseQuickSlot_17"),
        (0x10000137, "UseQuickSlot_18"),
        (0x10000138, "SelectQuickSlot_13"),
        (0x10000139, "SelectQuickSlot_14"),
        (0x1000013A, "SelectQuickSlot_15"),
        (0x1000013B, "SelectQuickSlot_16"),
        (0x1000013C, "SelectQuickSlot_17"),
        (0x1000013D, "SelectQuickSlot_18"),
        (0x1000013E, "PlayerOption_SideBySideVitals"),
        (0x1000013F, "PlayerOption_HearPKDeaths"),
    ];

    /// This client's own actions ([`super::dereth`]), by name, in id order.
    pub const DERETH_ACTION_NAMES: &[(u32, &str)] = &[
        (
            super::dereth::TOGGLE_PERFORMANCE_PANEL,
            "TogglePerformancePanel",
        ),
        (
            super::dereth::MOVEMENT_HOLD_SIDESTEP,
            "MovementHoldSidestep",
        ),
        (super::dereth::TOGGLE_TRADE_PANEL, "ToggleTradePanel"),
        (
            super::dereth::TOGGLE_SPELL_RESEARCH_PANEL,
            "ToggleSpellResearchPanel",
        ),
        (
            super::dereth::PLAYER_OPTION_AUTO_CREATE_SHORTCUTS,
            "PlayerOption_AutoCreateShortcuts",
        ),
        (
            super::dereth::TOGGLE_INVERT_MOUSE_LOOK,
            "ToggleInvertMouseLook",
        ),
        (
            super::dereth::TOGGLE_RIGHT_CLICK_MOUSE_LOOK,
            "ToggleRightClickMouseLook",
        ),
        (super::dereth::TOGGLE_STRETCH_UI, "ToggleStretchUI"),
        (
            super::dereth::TOGGLE_MUTE_ON_LOSING_FOCUS,
            "ToggleMuteOnLosingFocus",
        ),
    ];

    /// Every named action: the retail table, then this client's own.
    fn all() -> impl Iterator<Item = &'static (u32, &'static str)> {
        ACTION_ENUM_NAMES.iter().chain(DERETH_ACTION_NAMES)
    }

    /// The name the client writes for an action. An id the table does not know falls back to its
    /// decimal value, which is what an enum-mapper miss produces.
    #[must_use]
    pub fn enum_name_for_action(a: ActionId) -> String {
        all()
            .find(|(k, _)| *k == a.0)
            .map_or_else(|| a.0.to_string(), |(_, n)| (*n).to_owned())
    }

    /// The action a name stands for: the table's own name, or a decimal id.
    #[must_use]
    pub fn action_for_enum_name(name: &str) -> Option<ActionId> {
        all()
            .find(|(_, n)| *n == name)
            .map(|(k, _)| ActionId(*k))
            .or_else(|| name.parse().ok().map(ActionId))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_and_begin_are_starts_and_end_is_not() {
        let a = ActionId(0x29);
        assert!(Action::begin(a).is_start());
        assert!(Action::repeat(a, 2).is_start());
        assert!(!Action::end(a).is_start());
        assert!(Action::end(a).extent == 0.0);
        assert_eq!(Action::repeat(a, 2).repeats, 2);
    }

    #[test]
    fn a_name_round_trips_and_an_unknown_id_falls_back_to_decimal() {
        let forward = names::action_for_enum_name("MovementForward").expect("a shipped name");
        assert_eq!(forward, ActionId(0x29));
        assert_eq!(names::enum_name_for_action(forward), "MovementForward");
        assert_eq!(
            names::enum_name_for_action(ActionId(0x7FFF_FFFF)),
            "2147483647"
        );
        assert_eq!(names::action_for_enum_name("49"), Some(ActionId(49)));
        assert_eq!(names::action_for_enum_name("NoSuchAction"), None);
    }
}
