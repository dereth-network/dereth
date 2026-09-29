// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/AllegianceRank.cs
//! Port of `Source/ACE.Server/Entity/AllegianceRank.cs`.
//!
//! The static class `AllegianceTitle`: the allegiance rank title for a heritage and gender. An
//! unknown gender, heritage or rank gives `""`.

use empyrean_entity::enums::{Gender, HeritageGroup};

// ACE: AllegianceTitle.GetTitle
/// The title for `rank`, dispatched on gender, then heritage.
#[must_use]
pub fn get_title(heritage: HeritageGroup, gender: Gender, rank: u32) -> &'static str {
    match gender {
        Gender::Male => get_male_title(heritage, rank),
        Gender::Female => get_female_title(heritage, rank),
        _ => "",
    }
}

// ACE: AllegianceTitle.GetMaleTitle
/// The male title for `heritage` and `rank`.
#[must_use]
pub fn get_male_title(heritage: HeritageGroup, rank: u32) -> &'static str {
    match heritage {
        HeritageGroup::Aluvian => get_aluvian_male_title(rank),
        HeritageGroup::Gharundim => get_gharundim_male_title(rank),
        HeritageGroup::Sho => get_sho_male_title(rank),
        HeritageGroup::Viamontian => get_viamontian_male_title(rank),
        HeritageGroup::Shadowbound | HeritageGroup::Penumbraen => get_shadowbound_male_title(rank),
        HeritageGroup::Tumerok => get_tumerok_title(rank),
        HeritageGroup::Gearknight => get_gearknight_title(rank),
        HeritageGroup::Lugian => get_lugian_title(rank),
        HeritageGroup::Empyrean => get_empyrean_male_title(rank),
        HeritageGroup::Undead => get_undead_male_title(rank),
        _ => "",
    }
}

// ACE: AllegianceTitle.GetFemaleTitle
/// The female title for `heritage` and `rank`.
#[must_use]
pub fn get_female_title(heritage: HeritageGroup, rank: u32) -> &'static str {
    match heritage {
        HeritageGroup::Aluvian => get_aluvian_female_title(rank),
        HeritageGroup::Gharundim => get_gharundim_female_title(rank),
        HeritageGroup::Sho => get_sho_female_title(rank),
        HeritageGroup::Viamontian => get_viamontian_female_title(rank),
        HeritageGroup::Shadowbound | HeritageGroup::Penumbraen => {
            get_shadowbound_female_title(rank)
        }
        HeritageGroup::Tumerok => get_tumerok_title(rank),
        HeritageGroup::Gearknight => get_gearknight_title(rank),
        HeritageGroup::Lugian => get_lugian_title(rank),
        HeritageGroup::Empyrean => get_empyrean_female_title(rank),
        HeritageGroup::Undead => get_undead_female_title(rank),
        _ => "",
    }
}

// ACE: AllegianceTitle.GetAluvianMaleTitle
/// `GetAluvianMaleTitle(uint rank)`.
#[must_use]
pub fn get_aluvian_male_title(rank: u32) -> &'static str {
    match rank {
        1 => "Yeoman",
        2 => "Baronet",
        3 => "Baron",
        4 => "Reeve",
        5 => "Thane",
        6 => "Ealdor",
        7 => "Duke",
        8 => "Aetheling",
        9 => "King",
        10 => "High King",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetAluvianFemaleTitle
/// `GetAluvianFemaleTitle(uint rank)`.
#[must_use]
pub fn get_aluvian_female_title(rank: u32) -> &'static str {
    match rank {
        1 => "Yeoman",
        2 => "Baronet",
        3 => "Baroness",
        4 => "Reeve",
        5 => "Thane",
        6 => "Ealdor",
        7 => "Duchess",
        8 => "Aetheling",
        9 => "Queen",
        10 => "High Queen",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetGharundimMaleTitle
/// `GetGharundimMaleTitle(uint rank)`.
#[must_use]
pub fn get_gharundim_male_title(rank: u32) -> &'static str {
    match rank {
        1 => "Sayyid",
        2 => "Shayk",
        3 => "Maulan",
        4 => "Mu'allim",
        5 => "Naquib",
        6 => "Qadi",
        7 => "Mushir",
        8 => "Amir",
        9 => "Malik",
        10 => "Sultan",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetGharundimFemaleTitle
/// `GetGharundimFemaleTitle(uint rank)`.
#[must_use]
pub fn get_gharundim_female_title(rank: u32) -> &'static str {
    match rank {
        1 => "Sayyida",
        2 => "Shayka",
        3 => "Maulana",
        4 => "Mu'allima",
        5 => "Naquiba",
        6 => "Qadiya",
        7 => "Mushira",
        8 => "Amira",
        9 => "Malika",
        10 => "Sultana",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetShoMaleTitle
/// `GetShoMaleTitle(uint rank)`.
#[must_use]
pub fn get_sho_male_title(rank: u32) -> &'static str {
    match rank {
        1 => "Jinin",
        2 => "Jo-chueh",
        3 => "Nan-chueh",
        4 => "Shi-chueh",
        5 => "Ta-chueh",
        6 => "Kun-chueh",
        7 => "Kou",
        8 => "Taikou",
        9 => "Ou",
        10 => "Koutei",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetShoFemaleTitle
/// `GetShoFemaleTitle(uint rank)`.
#[must_use]
pub fn get_sho_female_title(rank: u32) -> &'static str {
    match rank {
        1 => "Jinin",
        2 => "Jo-chueh",
        3 => "Nan-chueh",
        4 => "Shi-chueh",
        5 => "Ta-chueh",
        6 => "Kun-chueh",
        7 => "Kou",
        8 => "Taikou",
        9 => "Jo-ou",
        10 => "Koutei",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetViamontianMaleTitle
/// `GetViamontianMaleTitle(uint rank)`.
#[must_use]
pub fn get_viamontian_male_title(rank: u32) -> &'static str {
    match rank {
        1 => "Squire",
        2 => "Banner",
        3 => "Baron",
        4 => "Viscount",
        5 => "Count",
        6 => "Marquis",
        7 => "Duke",
        8 => "Grand Duke",
        9 => "King",
        10 => "High King",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetViamontianFemaleTitle
/// `GetViamontianFemaleTitle(uint rank)`.
#[must_use]
pub fn get_viamontian_female_title(rank: u32) -> &'static str {
    match rank {
        1 => "Dame",
        2 => "Banner",
        3 => "Baroness",
        4 => "Viscountess",
        5 => "Countess",
        6 => "Marquise",
        7 => "Duchess",
        8 => "Grand Duchess",
        9 => "Queen",
        10 => "High Queen",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetShadowboundMaleTitle
/// `GetShadowboundMaleTitle(uint rank)`.
#[must_use]
pub fn get_shadowbound_male_title(rank: u32) -> &'static str {
    match rank {
        1 => "Tenebrous",
        2 => "Shade",
        3 => "Squire",
        4 => "Knight",
        5 => "Void Knight",
        6 => "Void Lord",
        7 => "Duke",
        8 => "Archduke",
        9 => "Highborn",
        10 => "King",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetShadowboundFemaleTitle
/// `GetShadowboundFemaleTitle(uint rank)`.
#[must_use]
pub fn get_shadowbound_female_title(rank: u32) -> &'static str {
    match rank {
        1 => "Tenebrous",
        2 => "Shade",
        3 => "Squire",
        4 => "Knight",
        5 => "Void Knight",
        6 => "Void Lady",
        7 => "Duchess",
        8 => "Archduchess",
        9 => "Highborn",
        10 => "Queen",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetTumerokTitle
/// `GetTumerokTitle(uint rank)`.
#[must_use]
pub fn get_tumerok_title(rank: u32) -> &'static str {
    match rank {
        1 => "Xutua",
        2 => "Tuona",
        3 => "Ona",
        4 => "Nuona",
        5 => "Turea",
        6 => "Rea",
        7 => "Nurea",
        8 => "Kauh",
        9 => "Sutah",
        10 => "Tah",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetGearknightTitle
/// `GetGearknightTitle(uint rank)`.
#[must_use]
pub fn get_gearknight_title(rank: u32) -> &'static str {
    match rank {
        1 => "Tribunus",
        2 => "Praefectus",
        3 => "Optio",
        4 => "Centurion",
        5 => "Principes",
        6 => "Legatus",
        7 => "Consul",
        8 => "Dux",
        9 => "Secondus",
        10 => "Primus",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetLugianTitle
/// `GetLugianTitle(uint rank)`.
#[must_use]
pub fn get_lugian_title(rank: u32) -> &'static str {
    match rank {
        1 => "Laigus",
        2 => "Raigus",
        3 => "Amploth",
        4 => "Arintoth",
        5 => "Obeloth",
        6 => "Lithos",
        7 => "Kantos",
        8 => "Gigas",
        9 => "Extas",
        10 => "Tiatus",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetEmpyreanMaleTitle
/// `GetEmpyreanMaleTitle(uint rank)`.
#[must_use]
pub fn get_empyrean_male_title(rank: u32) -> &'static str {
    match rank {
        1 => "Ensign",
        2 => "Corporal",
        3 => "Lieutenant",
        4 => "Commander",
        5 => "Captain",
        6 => "Commodore",
        7 => "Admiral",
        8 => "Warlord",
        9 => "Ipharsin",
        10 => "Aulin",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetEmpyreanFemaleTitle
/// `GetEmpyreanFemaleTitle(uint rank)`.
#[must_use]
pub fn get_empyrean_female_title(rank: u32) -> &'static str {
    match rank {
        1 => "Ensign",
        2 => "Corporal",
        3 => "Lieutenant",
        4 => "Commander",
        5 => "Captain",
        6 => "Commodore",
        7 => "Admiral",
        8 => "Warlord",
        9 => "Ipharsia",
        10 => "Aulia",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetUndeadMaleTitle
/// `GetUndeadMaleTitle(uint rank)`.
#[must_use]
pub fn get_undead_male_title(rank: u32) -> &'static str {
    match rank {
        1 => "Neophyte",
        2 => "Acolyte",
        3 => "Adept",
        4 => "Esquire",
        5 => "Squire",
        6 => "Knight",
        7 => "Count",
        8 => "Viscount",
        9 => "Highness",
        10 => "Annointed",
        _ => "",
    }
}

// ACE: AllegianceTitle.GetUndeadFemaleTitle
/// `GetUndeadFemaleTitle(uint rank)`.
#[must_use]
pub fn get_undead_female_title(rank: u32) -> &'static str {
    match rank {
        1 => "Neophyte",
        2 => "Acolyte",
        3 => "Adept",
        4 => "Esquire",
        5 => "Squire",
        6 => "Knight",
        7 => "Countess",
        8 => "Viscountess",
        9 => "Highness",
        10 => "Annointed",
        _ => "",
    }
}
