// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Appearance.cs
//! `Appearance`: the character-creation appearance choices.

use crate::binary_io::BinaryReader;

/// ACE: Appearance
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Appearance {
    // ACE: Appearance.Eyes
    pub eyes: u32,
    // ACE: Appearance.Nose
    pub nose: u32,
    // ACE: Appearance.Mouth
    pub mouth: u32,
    // ACE: Appearance.HairColor
    pub hair_color: u32,
    // ACE: Appearance.EyeColor
    pub eye_color: u32,
    // ACE: Appearance.HairStyle
    pub hair_style: u32,
    // ACE: Appearance.HeadgearStyle
    pub headgear_style: u32,
    // ACE: Appearance.HeadgearColor
    pub headgear_color: u32,
    // ACE: Appearance.ShirtStyle
    pub shirt_style: u32,
    // ACE: Appearance.ShirtColor
    pub shirt_color: u32,
    // ACE: Appearance.PantsStyle
    pub pants_style: u32,
    // ACE: Appearance.PantsColor
    pub pants_color: u32,
    // ACE: Appearance.FootwearStyle
    pub footwear_style: u32,
    // ACE: Appearance.FootwearColor
    pub footwear_color: u32,
    // ACE: Appearance.SkinHue
    pub skin_hue: f64,
    // ACE: Appearance.HairHue
    pub hair_hue: f64,
    // ACE: Appearance.HeadgearHue
    pub headgear_hue: f64,
    // ACE: Appearance.ShirtHue
    pub shirt_hue: f64,
    // ACE: Appearance.PantsHue
    pub pants_hue: f64,
    // ACE: Appearance.FootwearHue
    pub footwear_hue: f64,
}

impl Appearance {
    /// Reads the fields in declaration order. `None` is .NET's `EndOfStreamException`; the fields
    /// read before it keep their new values, as in ACE.
    // ACE: Appearance.Unpack
    pub fn unpack(&mut self, reader: &mut BinaryReader<'_>) -> Option<()> {
        self.eyes = reader.read_u32().ok()?;
        self.nose = reader.read_u32().ok()?;
        self.mouth = reader.read_u32().ok()?;
        self.hair_color = reader.read_u32().ok()?;
        self.eye_color = reader.read_u32().ok()?;
        self.hair_style = reader.read_u32().ok()?;
        self.headgear_style = reader.read_u32().ok()?;
        self.headgear_color = reader.read_u32().ok()?;
        self.shirt_style = reader.read_u32().ok()?;
        self.shirt_color = reader.read_u32().ok()?;
        self.pants_style = reader.read_u32().ok()?;
        self.pants_color = reader.read_u32().ok()?;
        self.footwear_style = reader.read_u32().ok()?;
        self.footwear_color = reader.read_u32().ok()?;
        self.skin_hue = reader.read_f64().ok()?;
        self.hair_hue = reader.read_f64().ok()?;
        self.headgear_hue = reader.read_f64().ok()?;
        self.shirt_hue = reader.read_f64().ok()?;
        self.pants_hue = reader.read_f64().ok()?;
        self.footwear_hue = reader.read_f64().ok()?;
        Some(())
    }
}
