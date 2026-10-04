//! Creation preview framing for each interface and body shape.
use crate::DisplayVariant;

/// Camera position for the selected body and face/body framing.
#[must_use]
pub fn camera(heritage: u32, face: bool, variant: DisplayVariant) -> [f32; 3] {
    match (heritage, face) {
        (12, true) => [0., -1.85, 1.85],
        (13, true) => [0., -3.05, 2.75],
        (7, true) => [0., -0.85, 1.65],
        (_, true) => [0., -0.55, 1.65],
        (12, false) => [0., -3.8, 1.15],
        (13, false) => [0., -5.7, 1.65],
        (_, false) => match variant {
            DisplayVariant::Classic => [0., -2.2, 1.1],
            DisplayVariant::Modern => [0., -2.5, 0.95],
        },
    }
}

/// Localized description of a heritage on the creation page.
#[must_use]
pub const fn heritage_description_token(heritage: u32) -> Option<&'static str> {
    Some(match heritage {
        1 => "ID_CharGen_AluvianText",
        2 => "ID_CharGen_GaruText",
        3 => "ID_CharGen_ShoText",
        4 => "ID_CharGen_ViaText",
        5 | 10 => "ID_CharGen_ShadText",
        6 => "ID_CharGen_GearText",
        7 => "ID_CharGen_AunTText",
        8 => "ID_CharGen_LugText",
        9 => "ID_CharGen_EmpText",
        11 => "ID_CharGen_UndText",
        12 => "ID_CharGen_OlthoiText",
        13 => "ID_CharGen_OlthoiAcidText",
        _ => return None,
    })
}

/// Localized description of a named creation template, indexed by its world template row.
#[must_use]
pub const fn profession_description_token(template: usize) -> Option<&'static str> {
    Some(match template {
        0 => "ID_CharGen_CustomText",
        1 => "ID_CharGen_BowText",
        2 => "ID_CharGen_SwashText",
        3 => "ID_CharGen_LifeText",
        4 => "ID_CharGen_WarText",
        5 => "ID_CharGen_WayText",
        6 => "ID_CharGen_SoldierText",
        _ => return None,
    })
}
