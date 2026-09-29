// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.DatLoader/FileTypes/ClothingTable.cs
//! `ClothingTable.GetIcon` and `GetVisualPriority`.

use dereth_assets::ClothingTable;
use dereth_primitives::DataId;
use empyrean_entity::enums::CoverageMask;

/// `HUMAN_MALE`, `GetVisualPriority`'s default setup.
pub const DEFAULT_VISUAL_PRIORITY_SETUP: u32 = 0x0200_0001;

/// ACE's `ClothingTable` helpers.
pub trait ClothingTableExt {
    /// The icon of sub-palette effect `pal_effect_idx`, or 0.
    fn get_icon(&self, pal_effect_idx: u32) -> u32;

    /// The coverage an item really has on `setup_id` (ACE's default is
    /// [`DEFAULT_VISUAL_PRIORITY_SETUP`]), from the body parts it replaces; `None` when the table
    /// has no entry for the setup.
    fn get_visual_priority(&self, setup_id: u32) -> Option<u32>;
}

impl ClothingTableExt for ClothingTable {
    // ACE: ClothingTable.GetIcon
    fn get_icon(&self, pal_effect_idx: u32) -> u32 {
        self.palette_templates
            .get(&pal_effect_idx)
            .map_or(0, |t| t.icon.0)
    }

    // ACE: ClothingTable.GetVisualPriority
    fn get_visual_priority(&self, setup_id: u32) -> Option<u32> {
        let effects = self.clothing_bases.get(&DataId(setup_id))?;
        let mut visual_priority = CoverageMask::default();
        for t in effects {
            match t.part_num {
                0 => visual_priority |= CoverageMask::OuterwearAbdomen,
                1 | 5 => visual_priority |= CoverageMask::OuterwearUpperLegs,
                2 | 6 => visual_priority |= CoverageMask::OuterwearLowerLegs,
                3 | 4 | 7 | 8 => visual_priority |= CoverageMask::Feet,
                9 => visual_priority |= CoverageMask::OuterwearChest,
                10 | 13 => visual_priority |= CoverageMask::OuterwearUpperArms,
                11 | 14 => visual_priority |= CoverageMask::OuterwearLowerArms,
                12 | 15 => visual_priority |= CoverageMask::Hands,
                16 => visual_priority |= CoverageMask::Head,
                _ => {} // Lots of things we don't care about
            }
        }
        Some(visual_priority.bits())
    }
}
