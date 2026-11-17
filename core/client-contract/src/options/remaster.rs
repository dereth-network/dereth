//! The words of the remaster's option: one box on every interface's Client page, off until the
//! player ticks it, drawn on the Vulkan and Direct3D 12 renderers; and its own boxes under it,
//! one for each thing it can draw past the game's own light, the ones any card draws ticked at
//! first, so ticking the remaster's box alone draws the whole of it.
//!
//! The name the player sees comes from [`NAME`] alone: the box's caption and tooltip, the chat
//! lines and the report's line are all made from it. The stored option is
//! [`super::names::fidelity::REMASTER`], which never spells the name shown, so a new name never
//! changes a player's settings.

use super::names::fidelity as names;

/// The name shown, as a macro so the captions below are made from it at compile time.
macro_rules! shown_name {
    () => {
        "Remaster"
    };
}

/// The remaster's name, as the player sees it.
pub const NAME: &str = shown_name!();

/// The box's caption.
pub const CAPTION: &str = concat!(shown_name!(), " (preview)");

/// The box's tooltip.
pub const NOTE: &str = concat!(
    "Draws the world with the ",
    shown_name!(),
    ", which looks as the game does today. Vulkan or Direct3D 12 renderer."
);

/// The attribution NVIDIA's DLSS license asks of an application that carries its library: the
/// SDK by its name and the NVIDIA mark the license names. Shown wherever the client shows its own
/// name and version, and under the remaster's boxes, in a build that carries the library.
pub const DLSS_ATTRIBUTION: &str = "NVIDIA DLSS \u{b7} NVIDIA RTX\u{2122}";

/// The heading the box sits under.
pub const HEADING: &str = shown_name!();

/// What the client carries that the remaster's rows answer to: NVIDIA's DLSS library, which a page
/// names under the rows, and an open art pack, without which the art pack's box draws nothing and
/// is left off the page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Carried {
    /// NVIDIA's DLSS library stands beside the client.
    pub dlss: bool,
    /// An art pack is open.
    pub art_pack: bool,
}

thread_local! {
    /// What the client carries, as its presentation said once it was up; nothing until then.
    static CARRIED: std::cell::Cell<Carried> = const {
        std::cell::Cell::new(Carried {
            dlss: false,
            art_pack: false,
        })
    };
}

/// Note what the client carries, once its presentation is up, for the pages that show the rows.
pub fn set_carried(carried: Carried) {
    CARRIED.set(carried);
}

/// What the client carries, as last noted ([`set_carried`]).
#[must_use]
pub fn carried() -> Carried {
    CARRIED.get()
}

/// Whether a page leaves the box `name` off, as it would draw nothing: the art pack's, while no
/// pack is open.
#[must_use]
pub fn left_off(name: &str) -> bool {
    name == names::REMASTER_ART_PACK && !carried().art_pack
}

/// Whether a page names NVIDIA's DLSS under the rows, as its license asks of a client that carries
/// the library.
#[must_use]
pub fn attributed() -> bool {
    carried().dlss
}

/// The red line at the top of the screen, for a player who ticks the box on any other renderer.
pub const REFUSED_RENDERER: &str =
    concat!(shown_name!(), " needs the Vulkan or Direct3D 12 renderer.");

/// The red line at the top of the screen, for a player who ticks the box on a Vulkan device started without it:
/// the next start has it.
pub const REFUSED_UNTIL_RESTART: &str = concat!(
    shown_name!(),
    " takes effect at the next start: restart to apply it."
);

/// The red line at the top of the screen, for a player whose graphics card cannot draw it.
pub const REFUSED_CARD: &str = concat!(shown_name!(), " needs a newer graphics card or driver.");

/// The red line at the top of the screen, for a player when it could not start, or stopped.
pub const FAILED: &str = concat!(shown_name!(), " could not draw, and is off.");

/// Whether the box `name` is ticked where a profile says nothing of it: one of the remaster's
/// own that any card draws ([`Feature::at_first`]). The remaster's box itself is not.
#[must_use]
pub fn ticked_at_first(name: &str) -> bool {
    FEATURES
        .iter()
        .any(|f| f.at_first && f.name.eq_ignore_ascii_case(name))
}

/// One of the remaster's own boxes, which take effect while its box is ticked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feature {
    pub name: &'static str,
    pub caption: &'static str,
    pub note: &'static str,
    /// Ticked where a profile says nothing of it: what it draws, any card draws. Tracing, the
    /// upscalers and the art pack wait for the player (the art pack's box is ticked by naming a
    /// pack).
    pub at_first: bool,
}

/// Whether a box draws nothing on this build's devices, so its row is left out: over Metal no ray
/// is traced and there is no DLSS.
#[must_use]
pub fn drawn_nothing_here(name: &str) -> bool {
    cfg!(target_os = "macos")
        && matches!(
            name,
            names::REMASTER_TRACED_SHADOWS
                | names::REMASTER_TRACED_OCCLUSION
                | names::REMASTER_DLSS
        )
}

/// The remaster's own boxes, in the order the page shows them.
pub const FEATURES: [Feature; 24] = [
    Feature {
        name: names::REMASTER_SHADOWS,
        caption: "Shadows",
        note: "The sun casts shadows from everything nearby.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_SKY,
        caption: "Sky",
        note: "A physical sky over the game's own.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_FOG,
        caption: "Fog",
        note: "Fog that lies in the air and catches the light.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_OCCLUSION,
        caption: "Ambient occlusion",
        note: "Creases and corners darken.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_INDOOR_LAMPS,
        caption: "Indoor lamps",
        note: "Lamps light rooms per pixel.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_OUTDOOR_LAMPS,
        caption: "Outdoor lamps",
        note: "Lamps light the night outdoors per pixel.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_BLOOM,
        caption: "HDR and bloom",
        note: "Bright light glows and the view adjusts to it.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_SMOOTHING,
        caption: "Smooth edges",
        note: "Edges neither stair-step nor crawl.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_WATER,
        caption: "Water",
        note: "Water reflects the world and ripples.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_PUDDLES,
        caption: "Puddles",
        note: "Puddles gather on the ground.",
        at_first: false,
    },
    Feature {
        name: names::REMASTER_RAIN,
        caption: "Rain and snow",
        note: "Rain and snow fall on the game's rainy days.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_DETAIL,
        caption: "Far detail",
        note: "Models keep their detail farther off and fade between levels.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_BOUNCE,
        caption: "Bounced light",
        note: "Light bounces off what it falls on.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_GRADE,
        caption: "Color grade",
        note: "Colors graded for the weather and the hour.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_AIR,
        caption: "Air and haze",
        note:
            "Distance fades into the air, shade takes the sky's light and low ground holds a haze.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_GRASS,
        caption: "Grass",
        note: "Grass grows over the ground near you and sways in the wind.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_FLOWERS,
        caption: "Flowers and pebbles",
        note: "Flowers grow in the grass and pebbles lie on bare ground.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_SWAY,
        caption: "Tree sway",
        note: "An art pack's trees sway in the wind.",
        at_first: true,
    },
    Feature {
        name: names::REMASTER_TRACED_SHADOWS,
        caption: "Ray traced shadows",
        note: "Shadows are traced, where the graphics card can.",
        at_first: false,
    },
    Feature {
        name: names::REMASTER_TRACED_OCCLUSION,
        caption: "Ray traced occlusion",
        note: "Creases and corners are traced, where the graphics card can.",
        at_first: false,
    },
    Feature {
        name: names::REMASTER_UPSCALE,
        caption: if cfg!(target_os = "macos") {
            "MetalFX upscaling"
        } else {
            "FSR upscaling"
        },
        note: "Draws fewer pixels and fills in the rest.",
        at_first: false,
    },
    Feature {
        name: names::REMASTER_DLSS,
        caption: "DLSS upscaling",
        note: "Upscales with NVIDIA DLSS where its DLL is beside the client, otherwise with FSR.",
        at_first: false,
    },
    Feature {
        name: names::REMASTER_ART_PACK,
        caption: "Art pack",
        note: "Draws the art pack's models and ground, when one is open.",
        at_first: false,
    },
    Feature {
        name: names::REMASTER_CLIFF_SHELVES,
        caption: "Cliff shelves",
        note: "Steep rock faces in the distance are drawn as ledges.",
        at_first: false,
    },
];

/// The render scale's caption.
pub const SCALE_CAPTION: &str = "Render scale";

/// The render scale's tooltip.
pub const SCALE_NOTE: &str = "How many of the pixels are drawn while upscaling.";

/// The shares the render scale offers, in percent, the whole screen first.
pub const SCALE_CHOICES: [u32; 5] = [100, 85, 75, 67, 50];

/// The share drawn until the player chooses another.
pub const SCALE_DEFAULT: u32 = 67;

/// The value a word stands for: a share in percent, with or without its sign, from the least the
/// render scale offers to all of the pixels. `None` for another name or another word.
#[must_use]
pub fn scale_value(name: &str, raw: &str) -> Option<i32> {
    if !name.eq_ignore_ascii_case(names::REMASTER_SCALE) {
        return None;
    }
    let share: u32 = raw.trim().trim_end_matches('%').trim_end().parse().ok()?;
    (50..=100)
        .contains(&share)
        .then(|| i32::try_from(share).ok())
        .flatten()
}

/// The words for a share of the render scale; `None` for any other name.
#[must_use]
pub fn scale_words(name: &str, v: &crate::PrefValue) -> Option<String> {
    if !name.eq_ignore_ascii_case(names::REMASTER_SCALE) {
        return None;
    }
    match v {
        crate::PrefValue::Int(i) => Some(format!("{i}%")),
        _ => None,
    }
}

/// The shares of the render scale a page offers, each as its words; `None` for any other name.
#[must_use]
pub fn scale_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !name.eq_ignore_ascii_case(names::REMASTER_SCALE) {
        return None;
    }
    Some(
        SCALE_CHOICES
            .iter()
            .map(|&share| super::store::Choice {
                label: format!("{share}%"),
                value: i32::try_from(share).unwrap_or(100),
            })
            .collect(),
    )
}

/// God Rays' caption.
pub const GOD_RAYS_CAPTION: &str = "God rays";

/// God Rays' tooltip.
pub const GOD_RAYS_NOTE: &str = "Sunlight shows in the air in shafts, most at dawn and dusk.";

/// God Rays' strengths, by their value.
pub const GOD_RAYS_CHOICES: [&str; 3] = ["Off", "Low", "High"];

/// God Rays' strength until the player chooses another: off.
pub const GOD_RAYS_DEFAULT: u32 = 0;

/// The value a word stands for: a strength by its name, or its number. `None` for another name or
/// another word.
#[must_use]
pub fn god_rays_value(name: &str, raw: &str) -> Option<i32> {
    if !name.eq_ignore_ascii_case(names::REMASTER_GOD_RAYS) {
        return None;
    }
    let raw = raw.trim();
    let at = GOD_RAYS_CHOICES
        .iter()
        .position(|c| c.eq_ignore_ascii_case(raw))
        .or_else(|| {
            raw.parse::<usize>()
                .ok()
                .filter(|&n| n < GOD_RAYS_CHOICES.len())
        })?;
    i32::try_from(at).ok()
}

/// The word for a strength of God Rays; `None` for any other name.
#[must_use]
pub fn god_rays_words(name: &str, v: &crate::PrefValue) -> Option<String> {
    if !name.eq_ignore_ascii_case(names::REMASTER_GOD_RAYS) {
        return None;
    }
    match v {
        crate::PrefValue::Int(i) => usize::try_from(*i)
            .ok()
            .and_then(|i| GOD_RAYS_CHOICES.get(i))
            .map(|w| (*w).to_owned()),
        _ => None,
    }
}

/// God Rays' strengths a page offers; `None` for any other name.
#[must_use]
pub fn god_rays_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !name.eq_ignore_ascii_case(names::REMASTER_GOD_RAYS) {
        return None;
    }
    Some(
        GOD_RAYS_CHOICES
            .iter()
            .enumerate()
            .map(|(i, label)| super::store::Choice {
                label: (*label).to_owned(),
                value: i32::try_from(i).unwrap_or(0),
            })
            .collect(),
    )
}

/// The scenery distance's caption.
pub const SCENERY_CAPTION: &str = "Scenery distance";

/// The scenery distance's tooltip.
pub const SCENERY_NOTE: &str = "How far around you trees and buildings stand.";

/// The scenery distances, by their value less one: how many rings of blocks round the player's
/// stand their trees, buildings and objects.
pub const SCENERY_CHOICES: [&str; 4] = ["Near", "Medium", "Far", "Very far"];

/// The scenery distance until the player chooses another: near, as the game draws it.
pub const SCENERY_DEFAULT: u32 = 1;

/// The value a word stands for: a distance by its name, or its number. `None` for another name
/// or another word.
#[must_use]
pub fn scenery_value(name: &str, raw: &str) -> Option<i32> {
    if !name.eq_ignore_ascii_case(names::REMASTER_SCENERY) {
        return None;
    }
    let raw = raw.trim();
    let at = SCENERY_CHOICES
        .iter()
        .position(|c| c.eq_ignore_ascii_case(raw))
        .map(|i| i + 1)
        .or_else(|| {
            raw.parse::<usize>()
                .ok()
                .filter(|n| (1..=SCENERY_CHOICES.len()).contains(n))
        })?;
    i32::try_from(at).ok()
}

/// The word for a scenery distance; `None` for any other name.
#[must_use]
pub fn scenery_words(name: &str, v: &crate::PrefValue) -> Option<String> {
    if !name.eq_ignore_ascii_case(names::REMASTER_SCENERY) {
        return None;
    }
    match v {
        crate::PrefValue::Int(i) => usize::try_from(*i)
            .ok()
            .and_then(|i| i.checked_sub(1))
            .and_then(|i| SCENERY_CHOICES.get(i))
            .map(|w| (*w).to_owned()),
        _ => None,
    }
}

/// The scenery distances a page offers; `None` for any other name.
#[must_use]
pub fn scenery_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !name.eq_ignore_ascii_case(names::REMASTER_SCENERY) {
        return None;
    }
    Some(
        SCENERY_CHOICES
            .iter()
            .enumerate()
            .map(|(i, label)| super::store::Choice {
                label: (*label).to_owned(),
                value: i32::try_from(i + 1).unwrap_or(1),
            })
            .collect(),
    )
}

/// Register the box with the value store, off, its own boxes as they are at first, and the
/// render scale, God Rays and the scenery distance at their defaults. The store's
/// load then reads the profile's `[Fidelity]` section into them, and the page reads and writes
/// them.
pub fn register() -> usize {
    let off = |name| {
        usize::from(super::store::register_preference(
            name,
            crate::PrefValue::Bool(ticked_at_first(name)),
            super::store::DataType::Bool,
        ))
    };
    let scale = usize::from(super::store::register_preference(
        names::REMASTER_SCALE,
        crate::PrefValue::Int(i32::try_from(SCALE_DEFAULT).unwrap_or(100)),
        super::store::DataType::UInt,
    ));
    let god_rays = usize::from(super::store::register_preference(
        names::REMASTER_GOD_RAYS,
        crate::PrefValue::Int(i32::try_from(GOD_RAYS_DEFAULT).unwrap_or(0)),
        super::store::DataType::UInt,
    ));
    let scenery = usize::from(super::store::register_preference(
        names::REMASTER_SCENERY,
        crate::PrefValue::Int(i32::try_from(SCENERY_DEFAULT).unwrap_or(1)),
        super::store::DataType::UInt,
    ));
    off(super::names::fidelity::REMASTER)
        + FEATURES.iter().map(|f| off(f.name)).sum::<usize>()
        + scale
        + god_rays
        + scenery
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// The remaster's box is off at first, and so are puddles, tracing, the upscalers and the art
    /// pack; what it draws on any card is ticked, so its box alone draws the rest of it.
    #[test]
    fn its_box_is_off_at_first_and_its_ordinary_boxes_are_ticked() {
        assert!(!ticked_at_first(names::REMASTER));
        for on in [
            names::REMASTER_SHADOWS,
            names::REMASTER_SKY,
            names::REMASTER_BOUNCE,
            names::REMASTER_SWAY,
        ] {
            assert!(ticked_at_first(on), "{on}");
        }
        for later in [
            names::REMASTER_PUDDLES,
            names::REMASTER_TRACED_SHADOWS,
            names::REMASTER_TRACED_OCCLUSION,
            names::REMASTER_UPSCALE,
            names::REMASTER_DLSS,
            names::REMASTER_ART_PACK,
        ] {
            assert!(!ticked_at_first(later), "{later}");
        }
        assert_eq!(FEATURES.iter().filter(|f| f.at_first).count(), 17);
    }

    /// Every line the player reads names the remaster by the one name, and the stored option
    /// does not spell it.
    #[test]
    fn every_line_names_it_by_the_one_name_and_the_stored_option_never_does() {
        for line in [
            CAPTION,
            NOTE,
            HEADING,
            REFUSED_RENDERER,
            REFUSED_UNTIL_RESTART,
            REFUSED_CARD,
            FAILED,
        ] {
            assert!(line.contains(NAME), "{line}");
        }
        assert_eq!(CAPTION, "Remaster (preview)");
        assert_eq!(super::super::names::fidelity::REMASTER, "Fidelity.Remaster");
    }

    /// The art pack's box is left off a page until the client says a pack is open, and DLSS is
    /// named only where the client says it carries the library; nothing is carried at first.
    #[test]
    fn the_art_pack_box_and_the_dlss_line_follow_what_the_client_carries() {
        assert_eq!(carried(), Carried::default());
        assert!(left_off(names::REMASTER_ART_PACK));
        assert!(!left_off(names::REMASTER_SHADOWS));
        assert!(!attributed());
        set_carried(Carried {
            dlss: true,
            art_pack: true,
        });
        assert!(!left_off(names::REMASTER_ART_PACK));
        assert!(attributed());
        set_carried(Carried::default());
    }

    /// The render scale reads and writes as a share in percent, offers the shares a page lists,
    /// and answers for no other option.
    #[test]
    fn the_render_scale_is_a_share_in_percent_and_answers_for_itself_alone() {
        assert_eq!(scale_value(names::REMASTER_SCALE, " 75 % "), Some(75));
        assert_eq!(scale_value(names::REMASTER_SCALE, "49"), None);
        assert_eq!(scale_value(names::REMASTER_SCALE, "big"), None);
        assert_eq!(scale_value(names::REMASTER, "75"), None);
        assert_eq!(
            scale_words(names::REMASTER_SCALE, &crate::PrefValue::Int(67)).as_deref(),
            Some("67%")
        );
        let rows = scale_rows(names::REMASTER_SCALE).expect("its shares");
        assert_eq!(rows.len(), SCALE_CHOICES.len());
        assert!(SCALE_CHOICES.contains(&SCALE_DEFAULT));
        assert_eq!(rows[0].label, "100%");
        assert!(scale_rows(names::REMASTER).is_none());
    }

    /// God Rays reads its strengths by name or number and writes them by name, is off at first,
    /// and answers for no other option.
    #[test]
    fn god_rays_is_off_low_or_high_and_answers_for_itself_alone() {
        let name = names::REMASTER_GOD_RAYS;
        assert_eq!(god_rays_value(name, " high "), Some(2));
        assert_eq!(god_rays_value(name, "Low"), Some(1));
        assert_eq!(god_rays_value(name, "0"), Some(0));
        assert_eq!(god_rays_value(name, "3"), None);
        assert_eq!(god_rays_value(name, "bright"), None);
        assert_eq!(god_rays_value(names::REMASTER_SCALE, "1"), None);
        assert_eq!(
            god_rays_words(name, &crate::PrefValue::Int(1)).as_deref(),
            Some("Low")
        );
        assert_eq!(god_rays_words(name, &crate::PrefValue::Int(5)), None);
        let rows = god_rays_rows(name).expect("its strengths");
        assert_eq!(
            rows.iter()
                .map(|r| (r.label.as_str(), r.value))
                .collect::<Vec<_>>(),
            [("Off", 0), ("Low", 1), ("High", 2)]
        );
        assert_eq!(GOD_RAYS_DEFAULT, 0);
        assert!(god_rays_rows(names::REMASTER).is_none());
    }

    /// The scenery distance reads its distances by name or number and writes them by name, is
    /// near at first, and answers for no other option.
    #[test]
    fn the_scenery_distance_is_near_to_very_far_and_answers_for_itself_alone() {
        let name = names::REMASTER_SCENERY;
        assert_eq!(scenery_value(name, " far "), Some(3));
        assert_eq!(scenery_value(name, "Very far"), Some(4));
        assert_eq!(scenery_value(name, "2"), Some(2));
        assert_eq!(scenery_value(name, "0"), None);
        assert_eq!(scenery_value(name, "5"), None);
        assert_eq!(scenery_value(names::REMASTER_GOD_RAYS, "1"), None);
        assert_eq!(
            scenery_words(name, &crate::PrefValue::Int(1)).as_deref(),
            Some("Near")
        );
        assert_eq!(scenery_words(name, &crate::PrefValue::Int(0)), None);
        assert_eq!(scenery_words(name, &crate::PrefValue::Int(5)), None);
        let rows = scenery_rows(name).expect("its distances");
        assert_eq!(
            rows.iter()
                .map(|r| (r.label.as_str(), r.value))
                .collect::<Vec<_>>(),
            [("Near", 1), ("Medium", 2), ("Far", 3), ("Very far", 4)]
        );
        assert_eq!(SCENERY_DEFAULT, 1);
        assert!(scenery_rows(names::REMASTER).is_none());
    }
}
