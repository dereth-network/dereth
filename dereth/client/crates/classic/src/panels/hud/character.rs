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
    let asc = dereth_client_contract::ctime::asctime(timestamp, offset);
    let p: Vec<_> = asc.split_whitespace().collect();
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| *m == p[1])
    .unwrap()
        + 1;
    let year = p[4].parse::<i32>().unwrap();
    format!(
        "{month:02}/{:02}/{:02} {}",
        p[2].parse::<u32>().unwrap(),
        year.rem_euclid(100),
        p[3]
    )
}
fn body(i: &CharacterInfo) -> String {
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
            .map(body)
            .unwrap_or_default();
        let text_w = (w - 34).max(1);
        let content = crate::renderer::measure_text_height("16-7", &text, text_w)
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
        let text = body(&i);
        assert!(text.contains("You've died twice."));
        assert!(text.contains("Innate Coordination: 10"));
        assert!(!text.contains("Luminance"));
        assert!(text.ends_with("You are not overburdened at this time.\n"));
    }
    #[test]
    fn dates_use_c_locale_and_local_offset() {
        assert_eq!(date(0, 0), "01/01/70 00:00:00");
        assert_eq!(date(0, -3600), "12/31/69 23:00:00");
    }
}
