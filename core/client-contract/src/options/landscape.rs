//! The landscape presentation options: which era's ground and which era's sky draw the world.
//!
//! These are this client's own options, not retail ones. A dat set from before Throne of Destiny
//! carries two regions, one its clients loaded when they drew in software and one they loaded
//! when they drew with 3D hardware; the later dat sets carry one. The three differ in their land
//! surface (how the ground is textured) and their sky, and any of the three can draw any world:
//! the cells give heights, a terrain type and road bits per vertex, numbered the same in every
//! era, and the region decides only how they look.
//!
//! Two preferences choose, [`GROUND`](crate::options::landscape::GROUND) and [`SKY`](crate::options::landscape::SKY), each holding a [`RegionStyle`](crate::options::landscape::RegionStyle) or
//! [`WORLD_DEFAULT`](crate::options::landscape::WORLD_DEFAULT) (the world's own). They are registered in the option value store beside the
//! retail ones ([`crate::options::store::init`]) as unsigned enumerations whose choice labels are literal
//! text, so any front end lists them from [`crate::options::store::choice_rows`] and writes them with
//! [`crate::view::UiRequest::SetPreference`], as it does a retail option. The client applies a
//! change live. A style whose files are not present is refused: the current one stays, and the
//! client shows [`RequiredFiles::ground_notice`](crate::options::landscape::RequiredFiles::ground_notice) or [`RequiredFiles::sky_notice`](crate::options::landscape::RequiredFiles::sky_notice).

use crate::view::PrefValue;

/// `[Render] Ground`: the land surface the ground is drawn with.
pub const GROUND: &str = "Render.Ground";
/// `[Render] Sky`: the sky, and the light and fog that come with it.
pub const SKY: &str = "Render.Sky";

/// The value both preferences hold for "the world's own": the hardware region of a world from
/// before Throne of Destiny (Legacy Blend and its sky), the one region of a later world (Modern).
pub const WORLD_DEFAULT: i32 = 0;

/// One of the three regions a ground or a sky can be drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RegionStyle {
    /// The region the clients before Throne of Destiny loaded when they drew in software. Its
    /// ground recolours a few shared pictures with each corner's terrain palette (palette
    /// shifting).
    LegacySoftware,
    /// The region the clients before Throne of Destiny loaded when they drew with 3D hardware.
    /// Its ground blends terrain textures through alpha masks (texture merging), at a 128-pixel
    /// base.
    LegacyHardware,
    /// The region of the end-of-retail dat set, whose ground texture-merges at a 512-pixel base.
    Modern,
}

impl RegionStyle {
    /// The three, in the order the options list them.
    pub const ALL: [Self; 3] = [Self::LegacySoftware, Self::LegacyHardware, Self::Modern];

    /// The preference value.
    #[must_use]
    pub const fn value(self) -> i32 {
        match self {
            Self::LegacySoftware => 1,
            Self::LegacyHardware => 2,
            Self::Modern => 3,
        }
    }

    /// The style a preference value names; `None` for [`WORLD_DEFAULT`] and anything else.
    #[must_use]
    pub const fn from_value(v: i32) -> Option<Self> {
        match v {
            1 => Some(Self::LegacySoftware),
            2 => Some(Self::LegacyHardware),
            3 => Some(Self::Modern),
            _ => None,
        }
    }

    /// The ground's name for it, as the options list it.
    #[must_use]
    pub const fn ground_label(self) -> &'static str {
        match self {
            Self::LegacySoftware => "Palette Shift",
            Self::LegacyHardware => "Legacy Blend",
            Self::Modern => "Modern Blend",
        }
    }

    /// The sky's name for it, as the options list it.
    #[must_use]
    pub const fn sky_label(self) -> &'static str {
        match self {
            Self::LegacySoftware => "Legacy Software",
            Self::LegacyHardware => "Legacy Hardware",
            Self::Modern => "Modern",
        }
    }

    /// The files it is read from.
    #[must_use]
    pub const fn required_files(self) -> RequiredFiles {
        match self {
            Self::LegacySoftware | Self::LegacyHardware => RequiredFiles::Legacy,
            Self::Modern => RequiredFiles::Modern,
        }
    }
}

/// Which dat set a style needs: the one from before Throne of Destiny, or a later one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequiredFiles {
    /// A `portal.dat` from before Throne of Destiny: the world's own, or one given for
    /// presentation beside a later world.
    Legacy,
    /// The end-of-retail files: the world's own, or the interface files beside an older world.
    Modern,
}

impl RequiredFiles {
    /// What the client shows when a ground style needing these files is chosen without them.
    #[must_use]
    pub const fn ground_notice(self) -> &'static str {
        match self {
            Self::Legacy => "This terrain mode requires legacy DATs",
            Self::Modern => "This terrain mode requires end-of-retail DATs",
        }
    }

    /// What the client shows when a sky needing these files is chosen without them.
    #[must_use]
    pub const fn sky_notice(self) -> &'static str {
        match self {
            Self::Legacy => "This sky requires legacy DATs",
            Self::Modern => "This sky requires end-of-retail DATs",
        }
    }
}

/// Which of the two preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Landscape {
    /// [`GROUND`].
    Ground,
    /// [`SKY`].
    Sky,
}

impl Landscape {
    /// The preference this is, by name, in any case.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case(GROUND) {
            Some(Self::Ground)
        } else if name.eq_ignore_ascii_case(SKY) {
            Some(Self::Sky)
        } else {
            None
        }
    }

    /// The preference name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ground => GROUND,
            Self::Sky => SKY,
        }
    }

    /// The row's caption on an options page.
    #[must_use]
    pub const fn caption(self) -> &'static str {
        match self {
            Self::Ground => "Terrain Mode",
            Self::Sky => "Sky Mode",
        }
    }

    /// A style's name for this preference.
    #[must_use]
    pub const fn label(self, style: RegionStyle) -> &'static str {
        match self {
            Self::Ground => style.ground_label(),
            Self::Sky => style.sky_label(),
        }
    }

    /// The notice for a refused style.
    #[must_use]
    pub const fn notice(self, files: RequiredFiles) -> &'static str {
        match self {
            Self::Ground => files.ground_notice(),
            Self::Sky => files.sky_notice(),
        }
    }

    /// How the preferences file spells a value: one word per choice.
    #[must_use]
    pub const fn file_word(self, style: Option<RegionStyle>) -> &'static str {
        match (self, style) {
            (_, None) => "World",
            (Self::Ground, Some(RegionStyle::LegacySoftware)) => "PaletteShift",
            (Self::Ground, Some(RegionStyle::LegacyHardware)) => "LegacyBlend",
            (Self::Ground, Some(RegionStyle::Modern)) => "ModernBlend",
            (Self::Sky, Some(RegionStyle::LegacySoftware)) => "LegacySoftware",
            (Self::Sky, Some(RegionStyle::LegacyHardware)) => "LegacyHardware",
            (Self::Sky, Some(RegionStyle::Modern)) => "Modern",
        }
    }
}

/// The caption of the [`WORLD_DEFAULT`] choice.
pub const WORLD_DEFAULT_LABEL: &str = "World Default";

/// A preferences-file value: `Some(None)` is the world's own, `Some(Some(style))` a style, and
/// `None` a value that names neither (which leaves the preference where it was).
///
/// Every spelling of either preference is read for both, in any case and with or without spaces:
/// the file words, the option labels, and the three this client wrote before the styles had
/// names. Of those, `software` was the palette-shift region and `later` the end-of-retail ground,
/// and `hardware` was the world's own hardware region, which is the world's default.
#[must_use]
pub fn parse(raw: &str) -> Option<Option<RegionStyle>> {
    let word: String = raw
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    Some(match word.as_str() {
        "world" | "worlddefault" | "default" | "hardware" => None,
        "paletteshift" | "legacysoftware" | "software" => Some(RegionStyle::LegacySoftware),
        "legacyblend" | "legacyhardware" => Some(RegionStyle::LegacyHardware),
        "modernblend" | "modern" | "later" => Some(RegionStyle::Modern),
        _ => return None,
    })
}

/// The value a preferences-file string stands for, for the store's load; `None` for a name that
/// is not one of the two, or a value it cannot read. A plain number is read as the value.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    Landscape::of(name)?;
    if let Ok(v) = raw.trim().parse::<i32>() {
        return (v == WORLD_DEFAULT || RegionStyle::from_value(v).is_some()).then_some(v);
    }
    parse(raw).map(|s| s.map_or(WORLD_DEFAULT, RegionStyle::value))
}

/// How the store's save writes the value; `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    let which = Landscape::of(name)?;
    let PrefValue::Int(v) = v else { return None };
    Some(which.file_word(RegionStyle::from_value(*v)).to_string())
}

/// The choices an options list shows for one of the two: the world's own, then the three styles,
/// with their captions. `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    let which = Landscape::of(name)?;
    let mut rows = vec![super::store::Choice {
        label: WORLD_DEFAULT_LABEL.to_string(),
        value: WORLD_DEFAULT,
    }];
    rows.extend(RegionStyle::ALL.iter().map(|s| super::store::Choice {
        label: which.label(*s).to_string(),
        value: s.value(),
    }));
    Some(rows)
}

/// Register both in the option value store, holding the world's own.
pub fn register() -> usize {
    [GROUND, SKY]
        .iter()
        .map(|n| {
            usize::from(super::store::register_preference(
                n,
                PrefValue::Int(WORLD_DEFAULT),
                super::store::DataType::UInt,
            ))
        })
        .sum()
}

/// The style a stored preference value names; `None` is the world's own.
#[must_use]
pub fn style_of(v: &PrefValue) -> Option<RegionStyle> {
    match v {
        PrefValue::Int(i) => RegionStyle::from_value(*i),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file words, the captions and the three older spellings all read; anything else does
    /// not.
    #[test]
    fn every_spelling_of_a_style_reads_and_the_older_three_keep_their_meaning() {
        for s in RegionStyle::ALL {
            for which in [Landscape::Ground, Landscape::Sky] {
                assert_eq!(parse(which.file_word(Some(s))), Some(Some(s)));
                assert_eq!(parse(which.label(s)), Some(Some(s)));
                assert_eq!(parse(&which.label(s).to_ascii_uppercase()), Some(Some(s)));
            }
        }
        assert_eq!(parse("software"), Some(Some(RegionStyle::LegacySoftware)));
        assert_eq!(parse("Later"), Some(Some(RegionStyle::Modern)));
        assert_eq!(parse("hardware"), Some(None));
        assert_eq!(parse("World Default"), Some(None));
        assert_eq!(parse("tod"), None);
        assert_eq!(parse_value(GROUND, "Legacy Blend"), Some(2));
        assert_eq!(parse_value(SKY, "3"), Some(3));
        assert_eq!(parse_value(SKY, "7"), None);
        assert_eq!(parse_value("Render.FieldOfView", "Modern"), None);
    }

    /// The save writes one word per value, which reads back as the same value.
    #[test]
    fn a_saved_value_reads_back_as_itself() {
        for name in [GROUND, SKY] {
            for v in [0, 1, 2, 3] {
                let text = convert_to_string(name, &PrefValue::Int(v)).expect("one of the two");
                assert_eq!(parse_value(name, &text), Some(v), "{name} {v} as {text}");
            }
        }
        assert_eq!(
            convert_to_string(GROUND, &PrefValue::Int(1)).as_deref(),
            Some("PaletteShift")
        );
        assert_eq!(
            convert_to_string(SKY, &PrefValue::Int(1)).as_deref(),
            Some("LegacySoftware")
        );
    }

    /// The list a front end shows: the world's own first, then the three in order.
    #[test]
    fn the_ground_choices_are_the_world_default_and_the_three_named_modes() {
        let rows = choice_rows(GROUND).expect("rows");
        let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "World Default",
                "Palette Shift",
                "Legacy Blend",
                "Modern Blend"
            ]
        );
        assert_eq!(
            rows.iter().map(|r| r.value).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        assert!(choice_rows("Display.Resolution").is_none());
    }
}
