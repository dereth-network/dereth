// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PropertyInt.cs

use crate::enums::*;

/// `System.Enum.GetName(typeof(E), value)` with ACE's `int` argument.
fn name<E: AceEnum>(value: i32) -> Option<String> {
    get_name::<E>(i64::from(value)).map(str::to_owned)
}

impl PropertyInt {
    /// The name of `value` in the enum this property holds, for display.
    // ACE: PropertyIntExtensions.GetValueEnumName
    pub fn get_value_enum_name(self, value: i32) -> Option<String> {
        match self {
            PropertyInt::ActivationResponse => return name::<ActivationResponse>(value),
            PropertyInt::AetheriaBitfield => return name::<AetheriaBitfield>(value),
            PropertyInt::AttackHeight => return name::<AttackHeight>(value),
            PropertyInt::AttackType => return name::<AttackType>(value),
            PropertyInt::Attuned => return name::<AttunedStatus>(value),
            PropertyInt::AmmoType => return name::<AmmoType>(value),
            PropertyInt::Bonded => return name::<BondedStatus>(value),
            PropertyInt::ChannelsActive | PropertyInt::ChannelsAllowed => {
                return name::<Channel>(value)
            }
            PropertyInt::CombatMode => return name::<CombatMode>(value),
            PropertyInt::DefaultCombatStyle | PropertyInt::AiAllowedCombatStyle => {
                return name::<CombatStyle>(value)
            }
            PropertyInt::CombatUse => return name::<CombatUse>(value),
            PropertyInt::ClothingPriority => return name::<CoverageMask>(value),
            PropertyInt::CreatureType
            | PropertyInt::SlayerCreatureType
            | PropertyInt::FoeType
            | PropertyInt::FriendType => return name::<CreatureType>(value),
            PropertyInt::DamageType | PropertyInt::ResistanceModifierType => {
                return name::<DamageType>(value)
            }
            PropertyInt::CurrentWieldedLocation | PropertyInt::ValidLocations => {
                return name::<EquipMask>(value)
            }
            PropertyInt::EquipmentSetId => return name::<EquipmentSet>(value),
            PropertyInt::Gender => return name::<Gender>(value),
            PropertyInt::GeneratorDestructionType | PropertyInt::GeneratorEndDestructionType => {
                return name::<GeneratorDestruct>(value)
            }
            PropertyInt::GeneratorTimeType => return name::<GeneratorTimeType>(value),
            PropertyInt::GeneratorType => return name::<GeneratorType>(value),
            PropertyInt::HeritageGroup | PropertyInt::HeritageSpecificArmor => {
                return name::<HeritageGroup>(value)
            }
            PropertyInt::HookType => return name::<HookType>(value),
            PropertyInt::HouseType => return name::<HouseType>(value),
            PropertyInt::ImbuedEffect
            | PropertyInt::ImbuedEffect2
            | PropertyInt::ImbuedEffect3
            | PropertyInt::ImbuedEffect4
            | PropertyInt::ImbuedEffect5 => return name::<ImbuedEffectType>(value),
            PropertyInt::HookItemType
            | PropertyInt::ItemType
            | PropertyInt::MerchandiseItemTypes
            | PropertyInt::TargetType => return name::<ItemType>(value),
            PropertyInt::ItemXpStyle => return name::<ItemXpStyle>(value),
            PropertyInt::MaterialType => return name::<MaterialType>(value),
            PropertyInt::PaletteTemplate => return name::<PaletteTemplate>(value),
            PropertyInt::PhysicsState => return name::<PhysicsState>(value),
            PropertyInt::HookPlacement
            | PropertyInt::Placement
            | PropertyInt::PCAPRecordedPlacement => return name::<Placement>(value),
            PropertyInt::PortalBitmask => return name::<PortalBitmask>(value),
            PropertyInt::PlayerKillerStatus => return name::<PlayerKillerStatus>(value),
            PropertyInt::BoosterEnum => return name::<PropertyAttribute2nd>(value),
            PropertyInt::ShowableOnRadar => return name::<RadarBehavior>(value),
            PropertyInt::RadarBlipColor => return name::<RadarColor>(value),
            PropertyInt::WeaponSkill
            | PropertyInt::WieldSkillType
            | PropertyInt::WieldSkillType2
            | PropertyInt::WieldSkillType3
            | PropertyInt::WieldSkillType4
            | PropertyInt::AppraisalItemSkill => return name::<Skill>(value),
            PropertyInt::AccountRequirements => return name::<SubscriptionStatus>(value),
            PropertyInt::SummoningMastery => return name::<SummoningMastery>(value),
            PropertyInt::UiEffects => return name::<UiEffects>(value),
            PropertyInt::ItemUseable => return name::<Usable>(value),
            PropertyInt::WeaponType => return name::<WeaponType>(value),
            PropertyInt::WieldRequirements
            | PropertyInt::WieldRequirements2
            | PropertyInt::WieldRequirements3
            | PropertyInt::WieldRequirements4 => return name::<WieldRequirement>(value),
            PropertyInt::GeneratorStartTime | PropertyInt::GeneratorEndTime => {
                return Some(unix_time_seconds_invariant(value));
            }
            PropertyInt::ArmorType => return name::<ArmorType>(value),
            PropertyInt::ParentLocation => return name::<ParentLocation>(value),
            PropertyInt::PlacementPosition => return name::<Placement>(value),
            PropertyInt::HouseStatus => return name::<HouseStatus>(value),
            PropertyInt::UseCreatesContractId => return name::<ContractId>(value),
            PropertyInt::Faction1Bits
            | PropertyInt::Faction2Bits
            | PropertyInt::Faction3Bits
            | PropertyInt::Hatred1Bits
            | PropertyInt::Hatred2Bits
            | PropertyInt::Hatred3Bits => return name::<FactionBits>(value),
            PropertyInt::UseRequiresSkill
            | PropertyInt::UseRequiresSkillSpec
            | PropertyInt::SkillToBeAltered => return name::<Skill>(value),
            PropertyInt::HookGroup => return name::<HookGroupType>(value),
            _ => {}
        }

        None
    }
}

/// `DateTimeOffset.FromUnixTimeSeconds(value).DateTime.ToString(CultureInfo.InvariantCulture)`:
/// the invariant culture's general pattern, `MM/dd/yyyy HH:mm:ss`, in UTC.
fn unix_time_seconds_invariant(value: i32) -> String {
    let secs = i64::from(value);
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{m:02}/{d:02}/{y:04} {:02}:{:02}:{:02}",
        sod / 3600,
        sod / 60 % 60,
        sod % 60
    )
}
