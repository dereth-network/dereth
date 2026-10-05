//! The character information page: its layout, its text, and the age, birth and death lines.
use super::*;
use crate::int::i32_from;
use dereth_client_contract::view::CharacterInfo;
use dereth_presentation::appraisal::insert_commas as comma;
use dereth_primitives::num::to_i32_f64;

#[derive(Debug)]
pub(super) struct Character {
    width: u32,
    height: u32,
    offset: i32,
}
impl Default for Character {
    fn default() -> Self {
        Self {
            width: 300,
            height: 362,
            offset: 0,
        }
    }
}
fn duration(age: i32) -> String {
    let mut seconds = age as u32;
    let mut parts = vec![];
    for (unit, name) in [(2_592_000, "mo"), (86_400, "d"), (3600, "h"), (60, "m")] {
        let v = seconds / unit;
        seconds %= unit;
        if v != 0 {
            parts.push(format!("{v}{name}"));
        }
    }
    parts.push(format!("{seconds}s"));
    parts.join(" ")
}
/// A date as the C library's `%c` writes it in the C locale; the classic interface never sets a
/// locale, so the month and day names are always English.
pub(super) fn date(timestamp: i64, offset: i32) -> String {
    let c = dereth_client_contract::ctime::broken_down(timestamp, offset);
    // This interface accepts years in the signed 32-bit range.
    let year = i32::try_from(c.year).expect("calendar year fits i32");
    format!(
        "{:02}/{:02}/{:02} {:02}:{:02}:{:02}",
        c.month,
        c.day,
        year.rem_euclid(100),
        c.hour,
        c.minute,
        c.second
    )
}
/// The string table the end-of-retail character sheet's rows are on.
const SHEET_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0001);

/// The character sheet's augmentation and luminance section, as the end-of-retail sheet composes
/// it out of the world's string tables: shown when the world's era has luminance or the innate
/// augmentations, and empty otherwise. A row the tables do not hold shows nothing.
pub fn augmentation_text(view: &dyn GameView, strings: &dyn dereth_text::StringResolver) -> String {
    let Some(info) = view.character_info() else {
        return String::new();
    };
    dereth_presentation::character::augmentation_section(
        &info,
        view.era_features(),
        &mut |token, values| {
            dereth_text::render_token(strings, SHEET_TABLE, token, values).unwrap_or_default()
        },
    )
}

/// The augmentation and luminance section kept for the sheet: the world's string tables, read
/// once, and the text composed again only when the character or the era changes.
#[derive(Debug, Default)]
pub struct AugmentationSheet {
    strings: Option<dereth_text::DatStringResolver<dereth_dat::RetailDatStore>>,
    seen: Option<(dereth_primitives::EraFeatures, Option<CharacterInfo>)>,
    text: String,
}
impl AugmentationSheet {
    /// The section for this frame's view, over the world's dats.
    pub fn text(
        &mut self,
        view: &dyn GameView,
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
    ) -> &str {
        let key = (view.era_features(), view.character_info());
        if self.seen.as_ref() != Some(&key) {
            let strings = self
                .strings
                .get_or_insert_with(|| dereth_text::DatStringResolver::new(store.clone()));
            self.text = augmentation_text(view, strings);
            self.seen = Some(key);
        }
        &self.text
    }
}

fn body(i: &CharacterInfo, augmentations: &str) -> String {
    use dereth_presentation::character::{regeneration_band, resist_band};
    let mut s = String::new();
    if let Some(t) = i.created {
        s.push_str(&format!(
            "You were born on {}.\n",
            date(t as i64, i.utc_offset_secs)
        ));
    }
    if let Some(t) = i.age {
        s.push_str(&format!("You have played for {}.\n", duration(t)));
    }
    s.push_str(&match i.num_deaths {
        0 => "You've never died!\n\n".into(),
        1 => "You've died only once!\n\n".into(),
        2 => "You've died twice.\n\n".into(),
        n => format!("You've died {} times.\n\n", comma(n)),
    });
    let resist = resist_band(i.strength.wrapping_add(i.endurance) as u32);
    let regen = regeneration_band(i.strength.wrapping_add(i.endurance.wrapping_mul(2)) as u32);
    s.push_str(&format!(
        "Natural Resistances: {resist}\nDrain Resistances: {resist}\nRegeneration Bonus: {regen}\n"
    ));
    for (name, n) in [
        "Strength",
        "Endurance",
        "Coordination",
        "Quickness",
        "Focus",
        "Self",
    ]
    .into_iter()
    .zip(i.innate)
    {
        s.push_str(&format!("Innate {name}: {}\n", comma(n)));
    }
    s.push_str(&format!(
        "Chess Rank: {}\nFishing Skill: {}\n\n",
        i.chess_rank, i.fishing_skill
    ));
    if !augmentations.is_empty() {
        s.push_str(augmentations);
        s.push('\n');
    }
    if i.load >= 1.0 {
        let penalty = (10 - to_i32_f64((2.0 - i.load as f64).clamp(0.0, 1.0) * 10.0)) * 10;
        s.push_str(&format!("You are currently overburdened by {} Burden Units.This is reducing your Run, Jump, Melee Defense and Missile Defense skills by {penalty}%.\n",i.encumbrance-i.capacity));
    } else {
        s.push_str("You are not overburdened at this time.\n");
    }
    s
}
impl Panel for Character {
    fn id(&self) -> &'static str {
        "character-info"
    }
    fn resize(&mut self, w: u32, h: u32) {
        self.width = w.max(50);
        self.height = h.max(50);
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let (w, h) = (self.width as i32, self.height as i32);
        let mut f = PanelFrame::new(self.width, self.height);
        image(&mut f, 0x06001398, rect(0, 0, w, h), true, None);
        image(&mut f, 0x0600127b, rect(0, 0, 276, 25), true, None);
        f.text_box(
            rect(2, 2, 272, 21),
            "Character Information",
            "16-7",
            INK,
            TextAlign::Center,
            false,
            Some([0, 0, 276, 25]),
        );
        button(
            &mut f,
            "close",
            rect(276, 0, 24, 25),
            [0x06001393, 0x06001394, 0x06001393],
        );
        let text = c
            .game
            .character_info()
            .as_ref()
            .map(|i| body(i, &c.classic.augmentations))
            .unwrap_or_default();
        let text_w = (w - 34).max(1);
        let content = c
            .resources
            .fonts
            .text_height("16-7", &text, text_w)
            .unwrap_or(i32_from(text.lines().count()) * 16);
        let max = (content - (h - 25)).max(0);
        let offset = self.offset.clamp(0, max);
        f.text_box(
            rect(10, 25 - offset, text_w, content),
            text,
            "16-7",
            INK,
            TextAlign::Left,
            true,
            Some([0, 25, w - 19, h]),
        );
        f.control(
            "scroll",
            rect(w - 19, 25, 21, h - 25),
            ControlKind::ScrollBar {
                min: 0,
                max,
                value: offset,
                page: h - 25,
                step: 16,
                vertical: true,
                arrow_size: 21,
                thumb_size: 20,
            },
            true,
        );
        f
    }
    fn event(&mut self, e: ControlEvent, _: &Context<'_>) -> Vec<PanelAction> {
        match e {
            ControlEvent::Activate(id) if id == "close" => vec![PanelAction::Close],
            ControlEvent::Scroll { id, value } if id == "scroll" => {
                self.offset = value.max(0);
                vec![]
            }
            _ => vec![],
        }
    }
}
#[cfg(test)]
mod tests {
    //! Behaviour: none (classic panel adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn duration_uses_months_not_years_or_weeks() {
        assert_eq!(duration(31_536_061), "12mo 5d 1m 1s");
        assert_eq!(duration(0), "0s");
    }
    #[test]
    fn character_body_has_legacy_sections_without_luminance() {
        let i = CharacterInfo {
            chess_rank: 1400,
            innate: [10; 6],
            num_deaths: 2,
            ..Default::default()
        };
        let text = body(&i, "");
        assert!(text.contains("You've died twice."));
        assert!(text.contains("Innate Coordination: 10"));
        assert!(!text.contains("Luminance"));
        assert!(text.ends_with("You are not overburdened at this time.\n"));
    }
    #[test]
    fn dates_use_c_locale_and_local_offset() {
        assert_eq!(date(0, 0), "01/01/70 00:00:00");
        assert_eq!(date(0, -3600), "12/31/69 23:00:00");
        assert_eq!(date(-1, 0), "12/31/69 23:59:59");
        assert_eq!(date(951_782_400, 0), "02/29/00 00:00:00");
    }

    #[test]
    #[should_panic(expected = "calendar year fits i32")]
    fn dates_reject_years_outside_the_signed_32_bit_range() {
        let _ = date(i64::MAX, 0);
    }

    /// Two rows of the end-of-retail sheet: the luminance header, and Strength's augmentation
    /// with its count variable. Every other row is missing.
    #[derive(Debug)]
    struct Rows;
    impl Rows {
        fn row(id: u32) -> Option<(Vec<String>, Vec<u32>)> {
            let hash = |s: &str| dereth_primitives::num::hash::str_hash(s.as_bytes());
            if id == hash("ID_CharacterInfo_Luminance_Header") {
                Some((vec!["Luminance:\\n".into()], vec![]))
            } else if id == hash("ID_CharacterInfo_Augmentation_Attribute_Strength") {
                Some((
                    vec!["Strength augmentations: ".into(), "\\n".into()],
                    vec![hash("NUM_AUGMENTATIONS")],
                ))
            } else {
                None
            }
        }
    }
    impl dereth_text::StringResolver for Rows {
        fn resolve_raw(&self, table: dereth_primitives::DataId, id: u32) -> Option<String> {
            self.resolve_variants_raw(table, id)?.into_iter().next()
        }
        fn resolve_variants_raw(
            &self,
            table: dereth_primitives::DataId,
            id: u32,
        ) -> Option<Vec<String>> {
            (table == SHEET_TABLE).then(|| Self::row(id))?.map(|r| r.0)
        }
        fn resolve_variables(&self, table: dereth_primitives::DataId, id: u32) -> Option<Vec<u32>> {
            (table == SHEET_TABLE).then(|| Self::row(id))?.map(|r| r.1)
        }
    }

    use dereth_client_contract::snapshot::GameSnapshot;

    fn world(era: dereth_primitives::EraId, luminance: Option<bool>) -> GameSnapshot {
        let mut info = CharacterInfo::default();
        info.aug_ints.insert(0xDA, 2);
        let mut announced = dereth_primitives::EraFeatureOverrides::default();
        if let Some(on) = luminance {
            announced.set("luminance", on);
        }
        GameSnapshot {
            character_info: Some(info),
            era: Some(dereth_client_contract::view::EraView {
                era,
                announced_features: announced,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn the_augmentation_section_follows_the_worlds_era() {
        use dereth_primitives::EraId;
        assert_eq!(
            augmentation_text(&world(EraId::Eor, None), &Rows),
            "Luminance:\nStrength augmentations: 2\n"
        );
        assert_eq!(
            augmentation_text(&world(EraId::Eor, Some(false)), &Rows),
            "Strength augmentations: 2\n",
            "without luminance the header goes and the augmentations stay"
        );
        assert_eq!(
            augmentation_text(&world(EraId::Infiltration, None), &Rows),
            "",
            "an era with neither system shows no section"
        );
        let i = CharacterInfo::default();
        let text = body(&i, "Strength augmentations: 2\n");
        assert!(
            text.contains("Fishing Skill: 0\n\nStrength augmentations: 2\n\nYou are not"),
            "the section sits between the skills and the load: {text:?}"
        );
    }
}
