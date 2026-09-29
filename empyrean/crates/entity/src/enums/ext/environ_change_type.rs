// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/EnvironChangeType.cs

use crate::enums::EnvironChangeType;

impl EnvironChangeType {
    // ACE: EnvironChangeTypeExtensions.IsFog
    pub fn is_fog(self) -> bool {
        self <= EnvironChangeType::BlackFog2
    }

    // ACE: EnvironChangeTypeExtensions.IsSound
    pub fn is_sound(self) -> bool {
        self >= EnvironChangeType::RoarSound
    }
}
