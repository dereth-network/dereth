// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PropertyGroupName.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PropertyGroupName.cs`; do not edit by hand

/// ACE enum `PropertyGroupName`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyGroupName(pub i32);

#[allow(non_upper_case_globals)]
impl PropertyGroupName {
    pub const Invalid: Self = Self(0);
    pub const UICoreButton: Self = Self(1);
    pub const UICoreBrowser: Self = Self(2);
    pub const UICoreColorPicker: Self = Self(3);
    pub const UICoreComboBox: Self = Self(4);
    pub const UICoreContextMenu: Self = Self(5);
    pub const UICoreDialog: Self = Self(6);
    pub const UICoreDragbar: Self = Self(7);
    pub const UICoreElement: Self = Self(8);
    pub const UICoreField: Self = Self(9);
    pub const UICoreFrame: Self = Self(10);
    pub const UICoreGroupBox: Self = Self(11);
    pub const UICoreListBox: Self = Self(12);
    pub const UICoreMenu: Self = Self(13);
    pub const UICoreMeter: Self = Self(14);
    pub const UICorePanel: Self = Self(15);
    pub const UICoreResizebar: Self = Self(16);
    pub const UICoreScrollable: Self = Self(17);
    pub const UICoreScrollbar: Self = Self(18);
    pub const UICoreText: Self = Self(19);
    pub const UICoreViewport: Self = Self(20);
    pub const GameplayOptions: Self = Self(22);
    pub const Wb_AllWorkspaces: Self = Self(23);
    pub const Wb_Avatar: Self = Self(24);
    pub const Wb_Camera: Self = Self(25);
    pub const Wb_CursorSelection: Self = Self(26);
    pub const Wb_DungeonWorkspace: Self = Self(27);
    pub const Wb_EntityWorkspace: Self = Self(28);
    pub const Wb_Grid: Self = Self(29);
    pub const Wb_Misc: Self = Self(30);
    pub const Wb_RenderOptions: Self = Self(31);
    pub const Wb_WorldWorkspace: Self = Self(32);
    pub const Tools: Self = Self(33);
    pub const Physics: Self = Self(34);
    pub const Ethereal: Self = Self(35);
    pub const Link: Self = Self(36);
    pub const PregameUI: Self = Self(0x10000001);
    pub const GameUI: Self = Self(0x10000002);
    pub const SmartBoxWrapper: Self = Self(0x10000003);
}

impl PropertyGroupName {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::UICoreButton, Self::UICoreBrowser, Self::UICoreColorPicker, Self::UICoreComboBox, Self::UICoreContextMenu, Self::UICoreDialog, Self::UICoreDragbar, Self::UICoreElement, Self::UICoreField, Self::UICoreFrame, Self::UICoreGroupBox, Self::UICoreListBox, Self::UICoreMenu, Self::UICoreMeter, Self::UICorePanel, Self::UICoreResizebar, Self::UICoreScrollable, Self::UICoreScrollbar, Self::UICoreText, Self::UICoreViewport, Self::GameplayOptions, Self::Wb_AllWorkspaces, Self::Wb_Avatar, Self::Wb_Camera, Self::Wb_CursorSelection, Self::Wb_DungeonWorkspace, Self::Wb_EntityWorkspace, Self::Wb_Grid, Self::Wb_Misc, Self::Wb_RenderOptions, Self::Wb_WorldWorkspace, Self::Tools, Self::Physics, Self::Ethereal, Self::Link, Self::PregameUI, Self::GameUI, Self::SmartBoxWrapper];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "UICoreButton", "UICoreBrowser", "UICoreColorPicker", "UICoreComboBox", "UICoreContextMenu", "UICoreDialog", "UICoreDragbar", "UICoreElement", "UICoreField", "UICoreFrame", "UICoreGroupBox", "UICoreListBox", "UICoreMenu", "UICoreMeter", "UICorePanel", "UICoreResizebar", "UICoreScrollable", "UICoreScrollbar", "UICoreText", "UICoreViewport", "GameplayOptions", "Wb_AllWorkspaces", "Wb_Avatar", "Wb_Camera", "Wb_CursorSelection", "Wb_DungeonWorkspace", "Wb_EntityWorkspace", "Wb_Grid", "Wb_Misc", "Wb_RenderOptions", "Wb_WorldWorkspace", "Tools", "Physics", "Ethereal", "Link", "PregameUI", "GameUI", "SmartBoxWrapper"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[34, 37, 21, 0, 35, 33, 36, 38, 32, 2, 1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31];
}

super::support::ace_enum!(PropertyGroupName, i32, plain);
super::support::ace_enum_from!(PropertyGroupName, i32 => i64);
