// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/CharacterCreateInfo.cs
//! `CharacterCreateInfo`: the character-creation request payload.

use crate::appearance::Appearance;
use crate::binary_io::BinaryReader;
use crate::enums::{HeritageGroup, SkillAdvancementClass};

/// ACE: CharacterCreateInfo. The `private set` properties are public fields here; only
/// [`CharacterCreateInfo::unpack`] writes them in ported code.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterCreateInfo {
    // ACE: CharacterCreateInfo.Heritage
    pub heritage: HeritageGroup,
    // ACE: CharacterCreateInfo.Gender
    pub gender: u32,
    // ACE: CharacterCreateInfo.Appearance
    pub appearance: Appearance,
    // ACE: CharacterCreateInfo.TemplateOption
    pub template_option: i32,
    // ACE: CharacterCreateInfo.StrengthAbility
    pub strength_ability: u32,
    // ACE: CharacterCreateInfo.EnduranceAbility
    pub endurance_ability: u32,
    // ACE: CharacterCreateInfo.CoordinationAbility
    pub coordination_ability: u32,
    // ACE: CharacterCreateInfo.QuicknessAbility
    pub quickness_ability: u32,
    // ACE: CharacterCreateInfo.FocusAbility
    pub focus_ability: u32,
    // ACE: CharacterCreateInfo.SelfAbility
    pub self_ability: u32,
    // ACE: CharacterCreateInfo.CharacterSlot
    pub character_slot: u32,
    // ACE: CharacterCreateInfo.ClassId
    pub class_id: u32,
    // ACE: CharacterCreateInfo.SkillAdvancementClasses
    pub skill_advancement_classes: Vec<SkillAdvancementClass>,
    /// `null` until unpacked.
    // ACE: CharacterCreateInfo.Name
    pub name: Option<String>,
    // ACE: CharacterCreateInfo.StartArea
    pub start_area: u32,
    // ACE: CharacterCreateInfo.IsAdmin
    pub is_admin: bool,
    // ACE: CharacterCreateInfo.IsSentinel
    pub is_sentinel: bool,
}

impl CharacterCreateInfo {
    /// Reads the payload in ACE's order. `None` is .NET's `EndOfStreamException` (or the
    /// `ArgumentException` `ReadString16L` can throw); the fields read before it keep their new
    /// values (and skills already read stay in the list), as in ACE.
    // ACE: CharacterCreateInfo.Unpack
    #[allow(clippy::cast_possible_wrap)]
    pub fn unpack(&mut self, reader: &mut BinaryReader<'_>) -> Option<()> {
        reader.skip(4); /* Unknown constant (1) */
        self.heritage = HeritageGroup(reader.read_u32().ok()? as i32);
        self.gender = reader.read_u32().ok()?;
        self.appearance.unpack(reader)?;
        self.template_option = reader.read_i32().ok()?;
        self.strength_ability = reader.read_u32().ok()?;
        self.endurance_ability = reader.read_u32().ok()?;
        self.coordination_ability = reader.read_u32().ok()?;
        self.quickness_ability = reader.read_u32().ok()?;
        self.focus_ability = reader.read_u32().ok()?;
        self.self_ability = reader.read_u32().ok()?;
        self.character_slot = reader.read_u32().ok()?;
        self.class_id = reader.read_u32().ok()?;
        let num_of_skills = reader.read_u32().ok()?;
        for _ in 0..num_of_skills {
            self.skill_advancement_classes
                .push(SkillAdvancementClass(reader.read_u32().ok()?));
        }
        self.name = Some(reader.read_string16l().ok()?);
        self.start_area = reader.read_u32().ok()?;
        self.is_admin = reader.read_u32().ok()? == 1;
        self.is_sentinel = reader.read_u32().ok()? == 1;
        Some(())
    }
}
