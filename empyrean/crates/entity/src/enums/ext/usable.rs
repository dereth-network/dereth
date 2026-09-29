// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Usable.cs

use crate::enums::Usable;

impl Usable {
    // ACE: UsableExtensions.GetSourceFlags
    pub fn get_source_flags(self) -> Usable {
        self & Usable::SourceMask
    }

    // ACE: UsableExtensions.GetTargetFlags
    pub fn get_target_flags(self) -> Usable {
        Usable(self.0 >> 16)
    }
}
