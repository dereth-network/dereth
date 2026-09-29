//! Vectors: shared rule constants and their ACE entity enum values in this module
//! Skill advancement class equals the rules' SAC/chargen class; PWD mirror ids are ACE's property
//! ids.
//! Fixture: enum values and synthetic entity records.

use std::collections::BTreeSet;

use dereth_rules as rules;
use empyrean_entity::enums::*;

fn ace_set<E: AceEnum>() -> BTreeSet<u64> {
    E::MEMBERS.iter().map(|e| e.key()).collect()
}

#[test]
fn skill_advancement_class_matches_the_rules_sac_and_chargen_class() {
    use rules::chargen::SkillAdvancementClass as Cg;
    use rules::skills::Sac;
    for (ace, sac, cg) in [
        (SkillAdvancementClass::Inactive, Sac::Undef, Cg::Inactive),
        (
            SkillAdvancementClass::Untrained,
            Sac::Untrained,
            Cg::Untrained,
        ),
        (SkillAdvancementClass::Trained, Sac::Trained, Cg::Trained),
        (
            SkillAdvancementClass::Specialized,
            Sac::Specialized,
            Cg::Specialized,
        ),
    ] {
        assert_eq!(Sac::from_raw(ace.0), sac, "{ace:?}");
        assert_eq!(ace.0, sac as u32, "{ace:?}");
        assert_eq!(ace.0, cg as u32, "{ace:?}");
    }
    assert_eq!(
        ace_set::<SkillAdvancementClass>().len(),
        4,
        "ACE has exactly the four classes"
    );
}

/// The pwd mirror ids are aces property ids.
#[test]
fn the_pwd_mirror_ids_are_aces_property_ids() {
    use rules::pwd_mirror::{MirrorStat as S, PwdField as F, PWD_MIRROR};
    use rules::weenie::bitfield as b;
    let int = |p: PropertyInt| (S::Int, u32::from(p.0));
    let did = |p: PropertyDataId| (S::DataId, u32::from(p.0));
    let iid = |p: PropertyInstanceId| (S::InstanceId, u32::from(p.0));
    let bool_ = |p: PropertyBool| (S::Bool, u32::from(p.0));
    let bit = |mask: u32, inverted: bool| F::Bit { mask, inverted };
    let want = [
        (int(PropertyInt::ItemType), F::ObjType),
        (int(PropertyInt::ClothingPriority), F::Priority),
        (int(PropertyInt::ItemsCapacity), F::ItemsCapacity),
        (int(PropertyInt::ContainersCapacity), F::ContainersCapacity),
        (int(PropertyInt::ValidLocations), F::ValidLocations),
        (int(PropertyInt::CurrentWieldedLocation), F::Location),
        (int(PropertyInt::MaxStackSize), F::MaxStackSize),
        (int(PropertyInt::StackSize), F::StackSize),
        (int(PropertyInt::ItemUseable), F::Useability),
        (int(PropertyInt::UiEffects), F::Effects),
        (int(PropertyInt::Value), F::Value),
        (int(PropertyInt::AmmoType), F::AmmoType),
        (int(PropertyInt::CombatUse), F::CombatUse),
        (int(PropertyInt::MaxStructure), F::MaxStructure),
        (int(PropertyInt::Structure), F::Structure),
        (int(PropertyInt::RadarBlipColor), F::BlipColor),
        (int(PropertyInt::ShowableOnRadar), F::RadarEnum),
        (int(PropertyInt::PlayerKillerStatus), F::PlayerKillerStatus),
        (int(PropertyInt::HookType), F::HookType),
        (int(PropertyInt::HookItemType), F::HookItemTypes),
        (did(PropertyDataId::Icon), F::IconId),
        (did(PropertyDataId::RestrictionEffect), F::PScript),
        (did(PropertyDataId::IconOverlay), F::IconOverlay),
        (did(PropertyDataId::IconUnderlay), F::IconUnderlay),
        (iid(PropertyInstanceId::Container), F::Container),
        (iid(PropertyInstanceId::Wielder), F::Wielder),
        (iid(PropertyInstanceId::Monarch), F::Monarch),
        (iid(PropertyInstanceId::HouseOwner), F::HouseOwner),
        (iid(PropertyInstanceId::PetOwner), F::PetOwner),
        (bool_(PropertyBool::Stuck), bit(b::STUCK, false)),
        (bool_(PropertyBool::Locked), bit(b::OPENABLE, true)),
        (bool_(PropertyBool::Inscribable), bit(b::INSCRIBABLE, false)),
        (bool_(PropertyBool::UiHidden), bit(b::UI_HIDDEN, false)),
        (
            bool_(PropertyBool::IgnoreHouseBarriers),
            bit(b::CELL_BARRIER_IMMUNE, false),
        ),
        (
            bool_(PropertyBool::HiddenAdmin),
            bit(b::HIDDEN_ADMIN, false),
        ),
    ];
    let got: Vec<_> = PWD_MIRROR
        .iter()
        .map(|r| ((r.stat, r.id), r.field))
        .collect();
    assert_eq!(got, want);
}
