//! The shared catalog of key bindings used by both interfaces.
//!
//! Every action a player can bind is a row here: the input map it is bound in, the group it
//! belongs to, where the classic interface's page lists it and under what label, and whether it
//! does anything in each interface. Both pages build from this table, each in its own style: the
//! retail page by tab and section, the classic page by its January 2005 categories. A row that
//! does nothing in an interface is not on that interface's page and has no default key there.
//!
//! The rows are the user-bindable entries of the shipped action map (an action and the input map
//! it is allowed in), this client's own actions, and Disable Most Weather Effects, which the
//! shipped action map has but does not let a player bind. Two kinds of shipped row are not
//! listed: the quickslots 10 to 18, which no shortcut bar shows, and the quest detail panel's
//! toggle, which opens no window.

use crate::{ActionId, InputMapId};

/// The input maps the rows are bound in.
pub use crate::maps as map;

/// What a row is about. Each page filters and orders the shared catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Movement,
    Camera,
    Combat,
    Selection,
    Panels,
    Chat,
    Shortcuts,
    CharacterOptions,
    Emotes,
    Other,
}

/// The classic page's categories, by their place in [`CATEGORIES`].
pub mod category {
    pub const CAMERA: usize = 0;
    pub const MOVEMENT: usize = 1;
    pub const COMBAT: usize = 2;
    pub const TARGETING: usize = 3;
    pub const ITEMS: usize = 4;
    pub const PANELS: usize = 5;
    pub const CHARACTER_OPTIONS: usize = 6;
    pub const MISCELLANEOUS: usize = 7;
    pub const CHAT_POSES: usize = 8;
    pub const PERMANENT: usize = 9;
}

/// The classic page's category headings, in the page's order.
pub const CATEGORIES: [&str; 10] = [
    "Camera Keys",
    "Movement Keys",
    "Combat Keys",
    "Targeting Keys",
    "Item Manipulation Keys",
    "Player Panel Keys",
    "Character Options Keys",
    "Miscellaneous Keys",
    "Chat Pose Keys",
    "Permanent Keys",
];

/// Which interface's page.
pub use dereth_client_contract::options::interface::Interface;

/// Why a row does nothing in one interface, and so is not on that interface's page.
pub mod why {
    pub const ALTERNATE_CAMERA: &str = "The classic interface has no alternate camera mode: its \
        camera keys are always the camera keys.";
    pub const FLOATING_CHAT: &str = "The classic interface has no floating chat windows.";
    pub const PLUGIN_MANAGER: &str = "The classic interface has no plugin manager window.";
    pub const COMPASS: &str = "The classic interface's radar is always shown.";
    pub const SIDE_BY_SIDE_VITALS: &str = "The classic interface's vitals have no side-by-side \
        layout.";
    pub const RIGHT_CLICK_MOUSE_LOOK: &str = "It turns the classic interface's right-click mouse \
        look on or off; this interface has no such mode.";
    pub const STRETCH_UI: &str = "It turns the classic interface's stretched layout on or off; \
        this interface has no such layout.";
    pub const AUTO_CREATE_SHORTCUTS: &str = "Only the classic interface makes a shortcut of an \
        item used.";
    pub const CANCEL: &str = "Escape is this interface's own cancel key, and it is not bound.";
    pub const REPEAT_MESSAGE: &str = "This interface's chat entry brings back earlier lines with \
        the Up and Down arrows.";
}

/// One row of both key pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// The input map the action is bound in.
    pub map: InputMapId,
    /// The action, by the name a key map file gives it.
    pub action_name: &'static str,
    pub group: Group,
    /// The classic page's category ([`category`]).
    pub category: usize,
    /// The classic page's label.
    pub label: &'static str,
    /// Why the row does nothing in the modern interface, when it does nothing there.
    pub modern_not_used: Option<&'static str>,
    /// Why the row does nothing in the classic interface, when it does nothing there.
    pub classic_not_used: Option<&'static str>,
}

impl Row {
    /// The action.
    ///
    /// # Panics
    /// A name the action tables do not know, which the table's own test rules out.
    #[must_use]
    pub fn action(&self) -> ActionId {
        dereth_client_contract::actions::names::action_for_enum_name(self.action_name)
            .expect("every row names a known action")
    }

    /// The input map the action is bound in.
    #[must_use]
    pub const fn input_map(&self) -> InputMapId {
        self.map
    }

    /// Whether the keyboard editor lists this action. Hidden actions remain bindable.
    #[must_use]
    pub fn shown(&self, interface: Interface) -> bool {
        self.not_used(interface).is_none()
            && !(interface == Interface::Modern
                && matches!(
                    self.action_name,
                    "ToggleInvertMouseLook"
                        | "ToggleMuteOnLosingFocus"
                        | "TogglePerformancePanel"
                        | "ToggleTradePanel"
                        | "ToggleSpellResearchPanel"
                        | "MovementHoldSidestep"
                ))
    }

    /// Why the row does nothing in `interface`, when it does nothing there.
    #[must_use]
    pub const fn not_used(&self, interface: Interface) -> Option<&'static str> {
        match interface {
            Interface::Modern => self.modern_not_used,
            Interface::Classic => self.classic_not_used,
        }
    }
}

/// The row of `action` in `map`, when both pages list it.
#[must_use]
pub fn find(map: InputMapId, action: ActionId) -> Option<&'static Row> {
    ROWS.iter().find(|r| r.map == map && r.action() == action)
}

/// The rows of `action`, in whichever map: the camera actions are in two.
pub fn rows_of(action: ActionId) -> impl Iterator<Item = &'static Row> {
    ROWS.iter().filter(move |r| r.action() == action)
}

/// The retail page's caption for a row the shipped string table does not name: Disable Most
/// Weather Effects, under the Character Options page's own words for it.
#[must_use]
pub fn modern_caption(map: InputMapId, action: ActionId) -> Option<&'static str> {
    (map == map::CHARACTER_OPTIONS
        && action
            == dereth_client_contract::actions::names::action_for_enum_name(
                "PlayerOption_DisableMostWeatherEffects",
            )
            .unwrap_or(ActionId(0)))
    .then_some("Disable Most Weather Effects")
}

/// The retail page's tab (action class) for a row the shipped action map gives none: Disable
/// Most Weather Effects, which goes with the other character options.
#[must_use]
pub fn modern_class(map: InputMapId, action: ActionId) -> Option<u32> {
    modern_caption(map, action).map(|_| crate::dereth::class::CHARACTER_SETTINGS)
}

use category as C;
use map as M;
use why as Why;
use Group as G;

const fn row(
    map: InputMapId,
    action_name: &'static str,
    group: Group,
    category: usize,
    label: &'static str,
) -> Row {
    Row {
        map,
        action_name,
        group,
        category,
        label,
        modern_not_used: None,
        classic_not_used: None,
    }
}

const fn row_not_used(
    map: InputMapId,
    action_name: &'static str,
    group: Group,
    category: usize,
    label: &'static str,
    modern_not_used: Option<&'static str>,
    classic_not_used: Option<&'static str>,
) -> Row {
    Row {
        map,
        action_name,
        group,
        category,
        label,
        modern_not_used,
        classic_not_used,
    }
}

/// Every row, by group, in the modern page's order within each.
#[rustfmt::skip]
pub const ROWS: &[Row] = &[
    row(M::MOVEMENT, "MovementForward", G::Movement, C::MOVEMENT, "Walk Forward"),
    row(M::MOVEMENT, "MovementBackup", G::Movement, C::MOVEMENT, "Walk Backwards"),
    row(M::MOVEMENT, "MovementStop", G::Movement, C::MOVEMENT, "Stop Moving"),
    row(M::MOVEMENT, "MovementStrafeRight", G::Movement, C::MOVEMENT, "Side Step Right"),
    row(M::MOVEMENT, "MovementStrafeLeft", G::Movement, C::MOVEMENT, "Side Step Left"),
    row(M::MOVEMENT, "MovementTurnRight", G::Movement, C::MOVEMENT, "Turn Right"),
    row(M::MOVEMENT, "MovementTurnLeft", G::Movement, C::MOVEMENT, "Turn Left"),
    row(M::MOVEMENT, "MovementRunLock", G::Movement, C::MOVEMENT, "Auto Run"),
    row(M::MOVEMENT, "MovementJump", G::Movement, C::MOVEMENT, "Jump"),
    row(M::MOVEMENT, "MovementWalkMode", G::Movement, C::MOVEMENT, "Hold Run"),
    row(M::MOVEMENT, "Ready", G::Movement, C::MISCELLANEOUS, "Stand"),
    row(M::MOVEMENT, "Crouch", G::Movement, C::MISCELLANEOUS, "Crouch"),
    row(M::MOVEMENT, "Sitting", G::Movement, C::MISCELLANEOUS, "Sit Down"),
    row(M::MOVEMENT, "Sleeping", G::Movement, C::MISCELLANEOUS, "Lie Down"),
    row(M::OWN, "MovementHoldSidestep", G::Movement, C::MOVEMENT, "Hold Sidestep"),
    row(M::CAMERA, "CameraMoveToward", G::Camera, C::CAMERA, "Camera Closer"),
    row(M::CAMERA, "CameraMoveAway", G::Camera, C::CAMERA, "Camera Farther"),
    row(M::CAMERA, "CameraRotateLeft", G::Camera, C::CAMERA, "Camera Left Rotate"),
    row(M::CAMERA, "CameraRotateRight", G::Camera, C::CAMERA, "Camera Right Rotate"),
    row(M::CAMERA, "CameraRotateUp", G::Camera, C::CAMERA, "Camera Raise"),
    row(M::CAMERA, "CameraRotateDown", G::Camera, C::CAMERA, "Camera Lower"),
    row(M::CAMERA, "CameraViewDefault", G::Camera, C::CAMERA, "Reset View"),
    row(M::CAMERA, "CameraViewFirstPerson", G::Camera, C::CAMERA, "First Person View"),
    row(M::CAMERA, "CameraViewLookDown", G::Camera, C::CAMERA, "Floor View"),
    row(M::CAMERA, "CameraViewMapMode", G::Camera, C::CAMERA, "Map View"),
    row(M::CAMERA, "CameraInstantMouseLook", G::Camera, C::CAMERA, "Shift View"),
    row_not_used(M::CAMERA, "CameraActivateAlternateMode", G::Camera, C::CAMERA, "Toggle Alternate Camera Mode", None, Some(Why::ALTERNATE_CAMERA)),
    row_not_used(M::CAMERA_ALTERNATE, "CameraMoveToward", G::Camera, C::CAMERA, "Zoom Camera In (alternate)", None, Some(Why::ALTERNATE_CAMERA)),
    row_not_used(M::CAMERA_ALTERNATE, "CameraMoveAway", G::Camera, C::CAMERA, "Zoom Camera Out (alternate)", None, Some(Why::ALTERNATE_CAMERA)),
    row_not_used(M::CAMERA_ALTERNATE, "CameraRotateLeft", G::Camera, C::CAMERA, "Rotate Camera Left (alternate)", None, Some(Why::ALTERNATE_CAMERA)),
    row_not_used(M::CAMERA_ALTERNATE, "CameraRotateRight", G::Camera, C::CAMERA, "Rotate Camera Right (alternate)", None, Some(Why::ALTERNATE_CAMERA)),
    row_not_used(M::CAMERA_ALTERNATE, "CameraRotateUp", G::Camera, C::CAMERA, "Rotate Camera Up (alternate)", None, Some(Why::ALTERNATE_CAMERA)),
    row_not_used(M::CAMERA_ALTERNATE, "CameraRotateDown", G::Camera, C::CAMERA, "Rotate Camera Down (alternate)", None, Some(Why::ALTERNATE_CAMERA)),
    row_not_used(M::CAMERA_ALTERNATE, "CameraViewDefault", G::Camera, C::CAMERA, "Move Camera to Default (alternate)", None, Some(Why::ALTERNATE_CAMERA)),
    row_not_used(M::CAMERA_ALTERNATE, "CameraViewFirstPerson", G::Camera, C::CAMERA, "First Person Camera (alternate)", None, Some(Why::ALTERNATE_CAMERA)),
    row_not_used(M::CAMERA_ALTERNATE, "CameraViewLookDown", G::Camera, C::CAMERA, "Overhead View (alternate)", None, Some(Why::ALTERNATE_CAMERA)),
    row_not_used(M::CAMERA_ALTERNATE, "CameraViewMapMode", G::Camera, C::CAMERA, "Map View (alternate)", None, Some(Why::ALTERNATE_CAMERA)),
    row(M::COMBAT, "CombatToggleCombat", G::Combat, C::COMBAT, "Combat Mode"),
    row(M::MELEE, "CombatDecreaseAttackPower", G::Combat, C::COMBAT, "Decrease Power"),
    row(M::MELEE, "CombatIncreaseAttackPower", G::Combat, C::COMBAT, "Increase Power"),
    row(M::MELEE, "CombatLowAttack", G::Combat, C::COMBAT, "Attack Low"),
    row(M::MELEE, "CombatMediumAttack", G::Combat, C::COMBAT, "Attack Medium"),
    row(M::MELEE, "CombatHighAttack", G::Combat, C::COMBAT, "Attack High"),
    row(M::MISSILE, "CombatDecreaseMissileAccuracy", G::Combat, C::COMBAT, "Decrease Accuracy"),
    row(M::MISSILE, "CombatIncreaseMissileAccuracy", G::Combat, C::COMBAT, "Increase Accuracy"),
    row(M::MISSILE, "CombatAimLow", G::Combat, C::COMBAT, "Aim Low"),
    row(M::MISSILE, "CombatAimMedium", G::Combat, C::COMBAT, "Aim Medium"),
    row(M::MISSILE, "CombatAimHigh", G::Combat, C::COMBAT, "Aim High"),
    row(M::MAGIC, "CombatCastCurrentSpell", G::Combat, C::COMBAT, "Cast Spell"),
    row(M::MAGIC, "CombatPrevSpell", G::Combat, C::COMBAT, "Previous Spell"),
    row(M::MAGIC, "CombatNextSpell", G::Combat, C::COMBAT, "Next Spell"),
    row(M::MAGIC, "CombatPrevSpellTab", G::Combat, C::COMBAT, "Previous Spelltab"),
    row(M::MAGIC, "CombatNextSpellTab", G::Combat, C::COMBAT, "Next Spelltab"),
    row(M::MAGIC, "UseSpellSlot_1", G::Combat, C::COMBAT, "Spell Slot 1"),
    row(M::MAGIC, "UseSpellSlot_2", G::Combat, C::COMBAT, "Spell Slot 2"),
    row(M::MAGIC, "UseSpellSlot_3", G::Combat, C::COMBAT, "Spell Slot 3"),
    row(M::MAGIC, "UseSpellSlot_4", G::Combat, C::COMBAT, "Spell Slot 4"),
    row(M::MAGIC, "UseSpellSlot_5", G::Combat, C::COMBAT, "Spell Slot 5"),
    row(M::MAGIC, "UseSpellSlot_6", G::Combat, C::COMBAT, "Spell Slot 6"),
    row(M::MAGIC, "UseSpellSlot_7", G::Combat, C::COMBAT, "Spell Slot 7"),
    row(M::MAGIC, "UseSpellSlot_8", G::Combat, C::COMBAT, "Spell Slot 8"),
    row(M::MAGIC, "UseSpellSlot_9", G::Combat, C::COMBAT, "Spell Slot 9"),
    row(M::MAGIC, "UseSpellSlot_10", G::Combat, C::COMBAT, "Spell Slot 10"),
    row(M::MAGIC, "UseSpellSlot_11", G::Combat, C::COMBAT, "Spell Slot 11"),
    row(M::MAGIC, "UseSpellSlot_12", G::Combat, C::COMBAT, "Spell Slot 12"),
    row(M::MAGIC, "CombatFirstSpell", G::Combat, C::COMBAT, "First Spell"),
    row(M::MAGIC, "CombatLastSpell", G::Combat, C::COMBAT, "Last Spell"),
    row(M::MAGIC, "CombatFirstSpellTab", G::Combat, C::COMBAT, "First Spell Tab"),
    row(M::MAGIC, "CombatLastSpellTab", G::Combat, C::COMBAT, "Last Spell Tab"),
    row(M::ITEM_SELECTION, "SelectionSelf", G::Selection, C::ITEMS, "Select Self"),
    row(M::ITEM_SELECTION, "SelectionPickUp", G::Selection, C::ITEMS, "Move Selected to Backpack"),
    row(M::ITEM_SELECTION, "SelectionSplitStack", G::Selection, C::ITEMS, "Split Selected"),
    row(M::ITEM_SELECTION, "SelectionPreviousSelection", G::Selection, C::TARGETING, "Previous Selection"),
    row(M::ITEM_SELECTION, "SelectionClosestCompassItem", G::Selection, C::TARGETING, "Closest Compass Item"),
    row(M::ITEM_SELECTION, "SelectionPreviousCompassItem", G::Selection, C::TARGETING, "Previous Compass Item"),
    row(M::ITEM_SELECTION, "SelectionNextCompassItem", G::Selection, C::TARGETING, "Next Compass Item"),
    row(M::ITEM_SELECTION, "SelectionClosestItem", G::Selection, C::TARGETING, "Closest Item"),
    row(M::ITEM_SELECTION, "SelectionPreviousItem", G::Selection, C::TARGETING, "Previous Item"),
    row(M::ITEM_SELECTION, "SelectionNextItem", G::Selection, C::TARGETING, "Next Item"),
    row(M::ITEM_SELECTION, "SelectionClosestMonster", G::Selection, C::TARGETING, "Closest Monster"),
    row(M::ITEM_SELECTION, "SelectionPreviousMonster", G::Selection, C::TARGETING, "Previous Monster"),
    row(M::ITEM_SELECTION, "SelectionNextMonster", G::Selection, C::TARGETING, "Next Monster"),
    row(M::ITEM_SELECTION, "SelectionLastAttacker", G::Selection, C::TARGETING, "Last Attacker"),
    row(M::ITEM_SELECTION, "SelectionClosestPlayer", G::Selection, C::TARGETING, "Closest Player"),
    row(M::ITEM_SELECTION, "SelectionPreviousPlayer", G::Selection, C::TARGETING, "Previous Player"),
    row(M::ITEM_SELECTION, "SelectionNextPlayer", G::Selection, C::TARGETING, "Next Player"),
    row(M::ITEM_SELECTION, "SelectionPreviousFellow", G::Selection, C::TARGETING, "Previous Fellow"),
    row(M::ITEM_SELECTION, "SelectionNextFellow", G::Selection, C::TARGETING, "Next Fellow"),
    row(M::ITEM_SELECTION, "SelectionUseClosestUnopenedCorpse", G::Selection, C::TARGETING, "Use Closest Unopened Corpse"),
    row(M::ITEM_SELECTION, "SelectionUseNextUnopenedCorpse", G::Selection, C::TARGETING, "Use Next Unopened Corpse"),
    row(M::ITEM_SELECTION, "SelectionGive", G::Selection, C::ITEMS, "Give Selected"),
    row(M::ITEM_SELECTION, "SelectionDrop", G::Selection, C::ITEMS, "Drop Selected"),
    row(M::ITEM_SELECTION, "SelectionMoveToMainPack", G::Selection, C::ITEMS, "Place Selected Object in Main Pack"),
    row(M::ITEM_SELECTION, "SelectionClosestUnopenedCorpse", G::Selection, C::TARGETING, "Select Closest Unopened Corpse"),
    row(M::ITEM_SELECTION, "SelectionNextUnopenedCorpse", G::Selection, C::TARGETING, "Select Next Unopened Corpse"),
    row(M::UI, "USE", G::Selection, C::ITEMS, "Use Selected"),
    row(M::UI, "SelectionExamine", G::Selection, C::ITEMS, "Examine Selected"),
    row(M::UI, "ToggleAbusePanel", G::Panels, C::PANELS, "Abuse Reporting Panel"),
    row(M::UI, "ToggleCharacterInfoPanel", G::Panels, C::PANELS, "Character Information Panel"),
    row(M::UI, "TogglePositiveEffectsPanel", G::Panels, C::PANELS, "Helpful Spells Panel"),
    row(M::UI, "ToggleNegativeEffectsPanel", G::Panels, C::PANELS, "Harmful Spells Panel"),
    row(M::UI, "ToggleLinkStatusPanel", G::Panels, C::PANELS, "Link Status Panel"),
    row(M::UI, "ToggleUrgentAssistancePanel", G::Panels, C::PANELS, "Urgent Assistance Panel"),
    row(M::UI, "ToggleVitaePanel", G::Panels, C::PANELS, "Vitae Panel"),
    row(M::UI, "ToggleSocialPanel", G::Panels, C::PANELS, "Social Panel"),
    row(M::UI, "ToggleAllegiancePanel", G::Panels, C::PANELS, "Allegiance Panel"),
    row(M::UI, "ToggleFellowshipPanel", G::Panels, C::PANELS, "Fellowship Panel"),
    row(M::UI, "ToggleSpellManagementPanel", G::Panels, C::PANELS, "Spell Management Panel"),
    row(M::UI, "ToggleSpellbookPanel", G::Panels, C::PANELS, "Spellbook Panel"),
    row(M::UI, "ToggleSpellComponentsPanel", G::Panels, C::PANELS, "Spell Components Panel"),
    row(M::UI, "ToggleSkillManagementPanel", G::Panels, C::PANELS, "Character Detail Panel"),
    row(M::UI, "ToggleAttributesPanel", G::Panels, C::PANELS, "Attributes Panel"),
    row(M::UI, "ToggleSkillsPanel", G::Panels, C::PANELS, "Skills Panel"),
    row(M::UI, "ToggleWorldPanel", G::Panels, C::PANELS, "World Panel"),
    row(M::UI, "ToggleMapPanel", G::Panels, C::PANELS, "Map Panel"),
    row(M::UI, "ToggleHousePanel", G::Panels, C::PANELS, "House Panel"),
    row(M::UI, "ToggleInventoryPanel", G::Panels, C::PANELS, "Inventory Panel"),
    row(M::UI, "ToggleOptionsPanel", G::Panels, C::PANELS, "Options Panel"),
    row(M::UI, "ToggleGameplayOptionsPanel", G::Panels, C::PANELS, "Gameplay Options Panel"),
    row(M::UI, "ToggleCharacterOptionsPanel", G::Panels, C::PANELS, "Character Options Panel"),
    row(M::UI, "ToggleConfigOptionsPanel", G::Panels, C::PANELS, "Sound And Graphics Panel"),
    row_not_used(M::UI, "ToggleRadarPanel", G::Panels, C::PANELS, "Compass Panel", None, Some(Why::COMPASS)),
    row(M::UI, "ToggleKeyboardPanel", G::Panels, C::PANELS, "Keyboard Configuration Panel"),
    row(M::UI, "ToggleFriendsPanel", G::Panels, C::PANELS, "Friends Panel"),
    row(M::UI, "ToggleCharacterTitlePanel", G::Panels, C::PANELS, "Character Title Panel"),
    row(M::UI, "ToggleJournalPanel", G::Panels, C::PANELS, "Journal Panel"),
    row(M::UI, "TogglePageListPanel", G::Panels, C::PANELS, "Journal Page List Panel"),
    row(M::UI, "ToggleContractsPanel", G::Panels, C::PANELS, "Contracts Panel"),
    row(M::OWN, "ToggleTradePanel", G::Panels, C::PANELS, "Trade Panel"),
    row(M::OWN, "ToggleSpellResearchPanel", G::Panels, C::PANELS, "Spell Research Panel"),
    row_not_used(M::UI, "ToggleFloatingChatWindow1", G::Chat, C::PANELS, "Toggle Floating Chat Window 1", None, Some(Why::FLOATING_CHAT)),
    row_not_used(M::UI, "ToggleFloatingChatWindow2", G::Chat, C::PANELS, "Toggle Floating Chat Window 2", None, Some(Why::FLOATING_CHAT)),
    row_not_used(M::UI, "ToggleFloatingChatWindow3", G::Chat, C::PANELS, "Toggle Floating Chat Window 3", None, Some(Why::FLOATING_CHAT)),
    row_not_used(M::UI, "ToggleFloatingChatWindow4", G::Chat, C::PANELS, "Toggle Floating Chat Window 4", None, Some(Why::FLOATING_CHAT)),
    row(M::CHAT, "MonarchReply", G::Chat, C::MISCELLANEOUS, "Monarch Reply"),
    row(M::CHAT, "PatronReply", G::Chat, C::MISCELLANEOUS, "Patron Reply"),
    row(M::CHAT, "Reply", G::Chat, C::MISCELLANEOUS, "Reply"),
    row(M::CHAT, "EnterChatMode", G::Chat, C::PERMANENT, "Enter Chat Mode"),
    row(M::CHAT, "START_COMMAND", G::Chat, C::MISCELLANEOUS, "Issue Slash Command"),
    row(M::CHAT, "TellSelected", G::Chat, C::MISCELLANEOUS, "Tell to Selected"),
    row(M::TOGGLE_CHAT_ENTRY, "ToggleChatEntry", G::Chat, C::PERMANENT, "Switch Chat Mode"),
    row(M::QUICKSLOTS, "UseQuickSlot_1", G::Shortcuts, C::PERMANENT, "Use Shortcut 1"),
    row(M::QUICKSLOTS, "UseQuickSlot_2", G::Shortcuts, C::PERMANENT, "Use Shortcut 2"),
    row(M::QUICKSLOTS, "UseQuickSlot_3", G::Shortcuts, C::PERMANENT, "Use Shortcut 3"),
    row(M::QUICKSLOTS, "UseQuickSlot_4", G::Shortcuts, C::PERMANENT, "Use Shortcut 4"),
    row(M::QUICKSLOTS, "UseQuickSlot_5", G::Shortcuts, C::PERMANENT, "Use Shortcut 5"),
    row(M::QUICKSLOTS, "UseQuickSlot_6", G::Shortcuts, C::PERMANENT, "Use Shortcut 6"),
    row(M::QUICKSLOTS, "UseQuickSlot_7", G::Shortcuts, C::PERMANENT, "Use Shortcut 7"),
    row(M::QUICKSLOTS, "UseQuickSlot_8", G::Shortcuts, C::PERMANENT, "Use Shortcut 8"),
    row(M::QUICKSLOTS, "UseQuickSlot_9", G::Shortcuts, C::PERMANENT, "Use Shortcut 9"),
    row(M::QUICKSLOTS, "SelectQuickSlot_1", G::Shortcuts, C::MISCELLANEOUS, "Select Shortcut 1"),
    row(M::QUICKSLOTS, "SelectQuickSlot_2", G::Shortcuts, C::MISCELLANEOUS, "Select Shortcut 2"),
    row(M::QUICKSLOTS, "SelectQuickSlot_3", G::Shortcuts, C::MISCELLANEOUS, "Select Shortcut 3"),
    row(M::QUICKSLOTS, "SelectQuickSlot_4", G::Shortcuts, C::MISCELLANEOUS, "Select Shortcut 4"),
    row(M::QUICKSLOTS, "SelectQuickSlot_5", G::Shortcuts, C::MISCELLANEOUS, "Select Shortcut 5"),
    row(M::QUICKSLOTS, "SelectQuickSlot_6", G::Shortcuts, C::MISCELLANEOUS, "Select Shortcut 6"),
    row(M::QUICKSLOTS, "SelectQuickSlot_7", G::Shortcuts, C::MISCELLANEOUS, "Select Shortcut 7"),
    row(M::QUICKSLOTS, "SelectQuickSlot_8", G::Shortcuts, C::MISCELLANEOUS, "Select Shortcut 8"),
    row(M::QUICKSLOTS, "SelectQuickSlot_9", G::Shortcuts, C::MISCELLANEOUS, "Select Shortcut 9"),
    row(M::QUICKSLOTS, "CreateShortcut", G::Shortcuts, C::PERMANENT, "Create Shortcut"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_AutoRepeatAttack", G::CharacterOptions, C::CHARACTER_OPTIONS, "Auto-Repeat Attacks"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_IgnoreAllegianceRequests", G::CharacterOptions, C::CHARACTER_OPTIONS, "Ignore Allegiance Requests"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_IgnoreFellowshipRequests", G::CharacterOptions, C::CHARACTER_OPTIONS, "Ignore Fellowship Requests"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_IgnoreTradeRequests", G::CharacterOptions, C::CHARACTER_OPTIONS, "Ignore Trade Requests"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_PersistentAtDay", G::CharacterOptions, C::CHARACTER_OPTIONS, "Always Daylight Outdoors"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_AllowGive", G::CharacterOptions, C::CHARACTER_OPTIONS, "Let Players Give You Items"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_ViewCombatTarget", G::CharacterOptions, C::CHARACTER_OPTIONS, "Auto-Track Combat Targets"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_ShowTooltips", G::CharacterOptions, C::CHARACTER_OPTIONS, "Display Tooltips"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_UseDeception", G::CharacterOptions, C::CHARACTER_OPTIONS, "Attempt To Deceive Players"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_ToggleRun", G::CharacterOptions, C::CHARACTER_OPTIONS, "Run As Default Movement"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_StayInChatMode", G::CharacterOptions, C::CHARACTER_OPTIONS, "Stay In Chat Mode After Send"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_AdvancedCombatUI", G::CharacterOptions, C::CHARACTER_OPTIONS, "Advanced Combat Interface"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_AutoTarget", G::CharacterOptions, C::CHARACTER_OPTIONS, "Auto-Target"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_VividTargetingIndicator", G::CharacterOptions, C::CHARACTER_OPTIONS, "Vivid Target Indicator"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_FellowshipShareXP", G::CharacterOptions, C::CHARACTER_OPTIONS, "Share Fellowship X P"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_AcceptLootPermits", G::CharacterOptions, C::CHARACTER_OPTIONS, "Accept Corpse Looting"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_FellowshipShareLoot", G::CharacterOptions, C::CHARACTER_OPTIONS, "Share Fellowship Loot"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_FellowshipAutoAcceptRequests", G::CharacterOptions, C::CHARACTER_OPTIONS, "Automatically Accept Fellowship Requests"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_CoordinatesOnRadar", G::CharacterOptions, C::CHARACTER_OPTIONS, "Show Radar Coordinates"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_SpellDuration", G::CharacterOptions, C::CHARACTER_OPTIONS, "Show Spell Durations"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisableHouseRestrictionEffects", G::CharacterOptions, C::CHARACTER_OPTIONS, "Disable House Effect"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DragItemOnPlayerOpensSecureTrade", G::CharacterOptions, C::CHARACTER_OPTIONS, "Drag Item to Player Opens Trade"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayAllegianceLogonNotifications", G::CharacterOptions, C::CHARACTER_OPTIONS, "Show Allegiance Logons"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_UseChargeAttack", G::CharacterOptions, C::CHARACTER_OPTIONS, "Use Charge Attack"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_UseCraftSuccessDialog", G::CharacterOptions, C::CHARACTER_OPTIONS, "Toggle Crafting Chance Of Success Dialog"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_HearAllegianceChat", G::CharacterOptions, C::CHARACTER_OPTIONS, "Allegiance Chat"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayDateOfBirth", G::CharacterOptions, C::CHARACTER_OPTIONS, "Allow Others to See Your Date of Birth"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayAge", G::CharacterOptions, C::CHARACTER_OPTIONS, "Allow Others to See Your Age"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayChessRank", G::CharacterOptions, C::CHARACTER_OPTIONS, "Allow Others to See Your Chess Rank"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayFishingSkill", G::CharacterOptions, C::CHARACTER_OPTIONS, "Allow Others to See Your Fishing Skill"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayNumberDeaths", G::CharacterOptions, C::CHARACTER_OPTIONS, "Allow Others to See Your Number of Deaths"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayTimeStamps", G::CharacterOptions, C::CHARACTER_OPTIONS, "Display Timestamps"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_SalvageMultiple", G::CharacterOptions, C::CHARACTER_OPTIONS, "Salvage Multiple Materials at Once"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_HearGeneralChat", G::CharacterOptions, C::CHARACTER_OPTIONS, "Listen to General Chat"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_HearTradeChat", G::CharacterOptions, C::CHARACTER_OPTIONS, "Listen to Trade Chat"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_HearLFGChat", G::CharacterOptions, C::CHARACTER_OPTIONS, "Listen to LFG Chat"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_HearRoleplayChat", G::CharacterOptions, C::CHARACTER_OPTIONS, "Listen to Roleplay Chat"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayNumberCharacterTitles", G::CharacterOptions, C::CHARACTER_OPTIONS, "Allow Others to See Your Number of Titles"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_MainPackPreferred", G::CharacterOptions, C::CHARACTER_OPTIONS, "Use Main Pack as Default for Picking Up Items"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_LeadMissileTargets", G::CharacterOptions, C::CHARACTER_OPTIONS, "Lead Missile Targets"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_UseFastMissiles", G::CharacterOptions, C::CHARACTER_OPTIONS, "Use Fast Missiles"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_FilterLanguage", G::CharacterOptions, C::CHARACTER_OPTIONS, "Filter Language"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_ConfirmVolatileRareUse", G::CharacterOptions, C::CHARACTER_OPTIONS, "Confirm Use of Rare Gems"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_HearSocietyChat", G::CharacterOptions, C::CHARACTER_OPTIONS, "Listen to Society Chat"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_ShowHelm", G::CharacterOptions, C::CHARACTER_OPTIONS, "Your helmet or head gear is visible."),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisableDistanceFog", G::CharacterOptions, C::CHARACTER_OPTIONS, "Turn off distance fog on the landscape."),
    row(M::CHARACTER_OPTIONS, "PlayerOption_ShowCloak", G::CharacterOptions, C::CHARACTER_OPTIONS, "Your cloak is visible."),
    row_not_used(M::CHARACTER_OPTIONS, "PlayerOption_SideBySideVitals", G::CharacterOptions, C::CHARACTER_OPTIONS, "Side By Side Vitals", None, Some(Why::SIDE_BY_SIDE_VITALS)),
    row_not_used(M::OWN, "PlayerOption_AutoCreateShortcuts", G::CharacterOptions, C::CHARACTER_OPTIONS, "Auto-Create Shortcuts", Some(Why::AUTO_CREATE_SHORTCUTS), None),
    row(M::OWN, "ToggleInvertMouseLook", G::CharacterOptions, C::CHARACTER_OPTIONS, "Invert Mouse Look"),
    row_not_used(M::OWN, "ToggleRightClickMouseLook", G::CharacterOptions, C::CHARACTER_OPTIONS, "Right Click To Mouse Look", Some(Why::RIGHT_CLICK_MOUSE_LOOK), None),
    row_not_used(M::OWN, "ToggleStretchUI", G::CharacterOptions, C::CHARACTER_OPTIONS, "Stretch U I", Some(Why::STRETCH_UI), None),
    row(M::OWN, "ToggleMuteOnLosingFocus", G::CharacterOptions, C::CHARACTER_OPTIONS, "Mute On Losing Focus"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisableMostWeatherEffects", G::CharacterOptions, C::CHARACTER_OPTIONS, "Disable Weather"),
    row(M::EMOTES, "AFKState", G::Emotes, C::CHAT_POSES, "Chat Pose \"A F K State\""),
    row(M::EMOTES, "Akimbo", G::Emotes, C::CHAT_POSES, "Chat Pose \"Akimbo\""),
    row(M::EMOTES, "ATOYOT", G::Emotes, C::CHAT_POSES, "Chat Pose \"ATOYOT State\""),
    row(M::EMOTES, "AkimboState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Akimbo State\""),
    row(M::EMOTES, "AtEaseState", G::Emotes, C::CHAT_POSES, "Chat Pose \"At Ease State\""),
    row(M::EMOTES, "Beckon", G::Emotes, C::CHAT_POSES, "Chat Pose \"Beckon\""),
    row(M::EMOTES, "BeSeeingYou", G::Emotes, C::CHAT_POSES, "Chat Pose \"Be Seeing You\""),
    row(M::EMOTES, "BlowKiss", G::Emotes, C::CHAT_POSES, "Chat Pose \"Blow Kiss\""),
    row(M::EMOTES, "BowDeep", G::Emotes, C::CHAT_POSES, "Chat Pose \"Bow Deep\""),
    row(M::EMOTES, "BowDeepState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Bow Deep State\""),
    row(M::EMOTES, "Cheer", G::Emotes, C::CHAT_POSES, "Chat Pose \"Cheer\""),
    row(M::EMOTES, "ClapHands", G::Emotes, C::CHAT_POSES, "Chat Pose \"Clap Hands\""),
    row(M::EMOTES, "ClapHandsState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Clap Hands State\""),
    row(M::EMOTES, "Cringe", G::Emotes, C::CHAT_POSES, "Chat Pose \"Cringe\""),
    row(M::EMOTES, "CrossArmsState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Cross Arms State\""),
    row(M::EMOTES, "Cry", G::Emotes, C::CHAT_POSES, "Chat Pose \"Cry\""),
    row(M::EMOTES, "CurtseyState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Curtsey State\""),
    row(M::EMOTES, "DrudgeDance", G::Emotes, C::CHAT_POSES, "Chat Pose \"Drudge Dance\""),
    row(M::EMOTES, "DrudgeDanceState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Drudge Dance State\""),
    row(M::EMOTES, "HaveASeat", G::Emotes, C::CHAT_POSES, "Chat Pose \"Have A Seat\""),
    row(M::EMOTES, "HaveASeatState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Have A Seat State\""),
    row(M::EMOTES, "HeartyLaugh", G::Emotes, C::CHAT_POSES, "Chat Pose \"Hearty Laugh\""),
    row(M::EMOTES, "Helper", G::Emotes, C::CHAT_POSES, "Chat Pose \"Helper\""),
    row(M::EMOTES, "Kneel", G::Emotes, C::CHAT_POSES, "Chat Pose \"Kneel\""),
    row(M::EMOTES, "KneelState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Kneel State\""),
    row(M::EMOTES, "Knock", G::Emotes, C::CHAT_POSES, "Chat Pose \"Knock\""),
    row(M::EMOTES, "Laugh", G::Emotes, C::CHAT_POSES, "Chat Pose \"Laugh\""),
    row(M::EMOTES, "LeanState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Lean State\""),
    row(M::EMOTES, "MeditateState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Meditate State\""),
    row(M::EMOTES, "MimeDrink", G::Emotes, C::CHAT_POSES, "Chat Pose \"Mime Drink\""),
    row(M::EMOTES, "MimeEat", G::Emotes, C::CHAT_POSES, "Chat Pose \"Mime Eat\""),
    row(M::EMOTES, "Mock", G::Emotes, C::CHAT_POSES, "Chat Pose \"Mock\""),
    row(M::EMOTES, "Nod", G::Emotes, C::CHAT_POSES, "Chat Pose \"Nod\""),
    row(M::EMOTES, "NudgeLeft", G::Emotes, C::CHAT_POSES, "Chat Pose \"Nudge Left\""),
    row(M::EMOTES, "NudgeRight", G::Emotes, C::CHAT_POSES, "Chat Pose \"Nudge Right\""),
    row(M::EMOTES, "Plead", G::Emotes, C::CHAT_POSES, "Chat Pose \"Plead\""),
    row(M::EMOTES, "PleadState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Plead State\""),
    row(M::EMOTES, "Point", G::Emotes, C::CHAT_POSES, "Chat Pose \"Point\""),
    row(M::EMOTES, "PointState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Point State\""),
    row(M::EMOTES, "PointDown", G::Emotes, C::CHAT_POSES, "Chat Pose \"Point Down\""),
    row(M::EMOTES, "PointDownState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Point Down State\""),
    row(M::EMOTES, "PointLeft", G::Emotes, C::CHAT_POSES, "Chat Pose \"Point Left\""),
    row(M::EMOTES, "PointLeftState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Point Left State\""),
    row(M::EMOTES, "PointRight", G::Emotes, C::CHAT_POSES, "Chat Pose \"Point Right\""),
    row(M::EMOTES, "PointRightState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Point Right State\""),
    row(M::EMOTES, "PossumState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Possum State\""),
    row(M::EMOTES, "Pray", G::Emotes, C::CHAT_POSES, "Chat Pose \"Pray\""),
    row(M::EMOTES, "PrayState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Pray State\""),
    row(M::EMOTES, "ReadState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Read State\""),
    row(M::EMOTES, "Salute", G::Emotes, C::CHAT_POSES, "Chat Pose \"Salute\""),
    row(M::EMOTES, "SaluteState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Salute State\""),
    row(M::EMOTES, "ScanHorizon", G::Emotes, C::CHAT_POSES, "Chat Pose \"Scan Horizon\""),
    row(M::EMOTES, "ScratchHead", G::Emotes, C::CHAT_POSES, "Chat Pose \"Scratch Head\""),
    row(M::EMOTES, "ScratchHeadState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Scratch Head State\""),
    row(M::EMOTES, "ShakeFist", G::Emotes, C::CHAT_POSES, "Chat Pose \"Shake Fist\""),
    row(M::EMOTES, "ShakeFistState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Shake Fist State\""),
    row(M::EMOTES, "ShakeHead", G::Emotes, C::CHAT_POSES, "Chat Pose \"Shake Head\""),
    row(M::EMOTES, "Shiver", G::Emotes, C::CHAT_POSES, "Chat Pose \"Shiver\""),
    row(M::EMOTES, "ShiverState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Shiver State\""),
    row(M::EMOTES, "Shoo", G::Emotes, C::CHAT_POSES, "Chat Pose \"Shoo\""),
    row(M::EMOTES, "Shrug", G::Emotes, C::CHAT_POSES, "Chat Pose \"Shrug\""),
    row(M::EMOTES, "SitState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Sit State\""),
    row(M::EMOTES, "SitBackState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Sit Back State\""),
    row(M::EMOTES, "SitCrossleggedState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Sit Crosslegged State\""),
    row(M::EMOTES, "Slouch", G::Emotes, C::CHAT_POSES, "Chat Pose \"Slouch\""),
    row(M::EMOTES, "SlouchState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Slouch State\""),
    row(M::EMOTES, "SmackHead", G::Emotes, C::CHAT_POSES, "Chat Pose \"Smack Head\""),
    row(M::EMOTES, "SnowAngelState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Snow Angel State\""),
    row(M::EMOTES, "Spit", G::Emotes, C::CHAT_POSES, "Chat Pose \"Spit\""),
    row(M::EMOTES, "Surrender", G::Emotes, C::CHAT_POSES, "Chat Pose \"Surrender\""),
    row(M::EMOTES, "SurrenderState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Surrender State\""),
    row(M::EMOTES, "TalktotheHandState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Talktothe Hand State\""),
    row(M::EMOTES, "TapFoot", G::Emotes, C::CHAT_POSES, "Chat Pose \"Tap Foot\""),
    row(M::EMOTES, "TapFootState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Tap Foot State\""),
    row(M::EMOTES, "Teapot", G::Emotes, C::CHAT_POSES, "Chat Pose \"Teapot\""),
    row(M::EMOTES, "ThinkerState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Thinker State\""),
    row(M::EMOTES, "WarmHands", G::Emotes, C::CHAT_POSES, "Chat Pose \"Warm Hands\""),
    row(M::EMOTES, "Wave", G::Emotes, C::CHAT_POSES, "Chat Pose \"Wave\""),
    row(M::EMOTES, "WaveState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Wave State\""),
    row(M::EMOTES, "WaveLow", G::Emotes, C::CHAT_POSES, "Chat Pose \"Wave Low\""),
    row(M::EMOTES, "WaveHigh", G::Emotes, C::CHAT_POSES, "Chat Pose \"Wave High\""),
    row(M::EMOTES, "Winded", G::Emotes, C::CHAT_POSES, "Chat Pose \"Winded\""),
    row(M::EMOTES, "WindedState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Winded State\""),
    row(M::EMOTES, "Woah", G::Emotes, C::CHAT_POSES, "Chat Pose \"Woah\""),
    row(M::EMOTES, "WoahState", G::Emotes, C::CHAT_POSES, "Chat Pose \"Woah State\""),
    row(M::EMOTES, "YawnStretch", G::Emotes, C::CHAT_POSES, "Chat Pose \"Yawn Stretch\""),
    row(M::EMOTES, "YMCA", G::Emotes, C::CHAT_POSES, "Chat Pose \"YMCA\""),
    row(M::UI, "CaptureScreenshot", G::Other, C::MISCELLANEOUS, "Capture Screenshot To File"),
    row(M::UI, "ToggleHelp", G::Other, C::PERMANENT, "Help"),
    row_not_used(M::UI, "TogglePluginManager", G::Other, C::PANELS, "Display Plugin Manager", None, Some(Why::PLUGIN_MANAGER)),
    row(M::UI, "LOGOUT", G::Other, C::MISCELLANEOUS, "End Character Session"),
    row(M::OWN, "TogglePerformancePanel", G::Other, C::MISCELLANEOUS, "Performance Panel"),
    row_not_used(M::OWN, "Cancel", G::Other, C::PERMANENT, "Cancel / Options Panel", Some(Why::CANCEL), None),
    row_not_used(M::OWN, "RepeatLastMessage", G::Chat, C::PERMANENT, "Repeat Message (in chat box)", Some(Why::REPEAT_MESSAGE), None),
];

#[cfg(test)]
mod tests {
    //! Behaviour: none (the table's own consistency; the shipped action map is checked against it
    //! in the dat tier).
    use super::*;

    #[test]
    fn every_row_names_a_known_action_once_in_its_map() {
        let mut seen = std::collections::BTreeSet::new();
        for r in ROWS {
            let _ = r.action();
            assert!(
                seen.insert((r.map, r.action().0)),
                "{} twice",
                r.action_name
            );
            assert!(r.category < CATEGORIES.len(), "{}", r.action_name);
            assert!(!r.label.is_empty(), "{}", r.action_name);
        }
        assert_eq!(ROWS.len(), 308);
    }

    #[test]
    fn the_rows_each_interface_does_nothing_with_are_the_ones_it_has_no_counterpart_for() {
        let modern: Vec<&str> = ROWS
            .iter()
            .filter(|r| r.not_used(Interface::Modern).is_some())
            .map(|r| r.action_name)
            .collect();
        assert_eq!(
            modern,
            [
                "PlayerOption_AutoCreateShortcuts",
                "ToggleRightClickMouseLook",
                "ToggleStretchUI",
                "Cancel",
                "RepeatLastMessage",
            ]
        );
        let classic = ROWS
            .iter()
            .filter(|r| r.not_used(Interface::Classic).is_some())
            .count();
        // The alternate camera mode and its ten keys, the four floating chat windows, the plugin
        // manager, the compass and side-by-side vitals.
        assert_eq!(classic, 18);
        assert!(ROWS
            .iter()
            .all(|r| r.not_used(Interface::Modern).is_none()
                || r.not_used(Interface::Classic).is_none()));
    }

    #[test]
    fn the_hidden_quickslots_and_the_quest_detail_panel_are_not_rows() {
        for name in [
            "UseQuickSlot_10",
            "UseQuickSlot_18",
            "ToggleQuestManagementPanel",
        ] {
            assert!(ROWS.iter().all(|r| r.action_name != name), "{name}");
        }
        assert!(ROWS.iter().any(|r| r.action_name == "UseQuickSlot_9"));
    }
    #[test]
    fn option_rows_agree_with_the_sheet_without_equating_visibility_and_support() {
        use dereth_client_contract::{options::sheet, view::PlayerOption};
        for key in ROWS.iter().filter(|r| r.group == Group::CharacterOptions) {
            let Some(name) = key.action_name.strip_prefix("PlayerOption_") else {
                continue;
            };
            let sheet_row = if name == "AutoCreateShortcuts" {
                sheet::PAGES
                    .iter()
                    .flat_map(|p| p.headings)
                    .flat_map(|h| h.rows)
                    .find(|r| r.value == sheet::Value::Bit { mask: 1 })
                    .expect("shortcut option")
            } else {
                let option = PlayerOption::ALL
                    .into_iter()
                    .find(|o| format!("{o:?}") == name)
                    .expect("named player option");
                sheet::row_of_option(option).expect("option on the sheet")
            };
            for interface in Interface::ALL {
                assert_eq!(
                    sheet_row.shown.on(interface),
                    key.not_used(interface).is_none(),
                    "{} {interface:?}",
                    key.action_name
                );
            }
        }
        for (action, preference) in [
            (
                "ToggleStretchUI",
                dereth_client_contract::options::classic::STRETCH_UI,
            ),
            (
                "ToggleRightClickMouseLook",
                dereth_client_contract::options::classic::RIGHT_CLICK_MOUSE_LOOK,
            ),
        ] {
            let row = sheet::row_of_preference(sheet::PageId::Client, preference)
                .expect("classic preference");
            let key = ROWS
                .iter()
                .find(|r| r.action_name == action)
                .expect("key row");
            assert_eq!(
                row.shown.on(Interface::Classic),
                key.not_used(Interface::Classic).is_none()
            );
            assert_eq!(
                row.shown.on(Interface::Modern),
                key.not_used(Interface::Modern).is_none()
            );
        }
        // These are supported controls deliberately omitted only from the Modern key editor.
        for name in [
            "ToggleInvertMouseLook",
            "ToggleMuteOnLosingFocus",
            "TogglePerformancePanel",
            "ToggleTradePanel",
            "ToggleSpellResearchPanel",
            "MovementHoldSidestep",
        ] {
            let key = ROWS
                .iter()
                .find(|r| r.action_name == name)
                .expect("hidden key row");
            assert_eq!(key.not_used(Interface::Modern), None);
            assert!(!key.shown(Interface::Modern));
        }
        // Era capabilities affect the options page; they do not delete the binding vocabulary.
        let cloak = sheet::row_of_option(PlayerOption::ShowCloak).expect("cloak option");
        assert_eq!(cloak.needs, sheet::Needs::Cloaks);
        assert!(ROWS
            .iter()
            .any(|r| r.action_name == "PlayerOption_ShowCloak"));
    }
}
