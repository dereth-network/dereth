//! The character information panel: attributes, vitals and skills, and raising or training them.
use super::super::*;
use super::common::*;
use crate::int::i32_from;
use dereth_client_contract::{PkStatus, SkillEntry};
use dereth_primitives::num::to_i32;

const ATTRIBUTES: [(u32, bool, &str, u32); 9] = [
    (1, false, "Strength", 0x060002c8),
    (2, false, "Endurance", 0x060002c4),
    (4, false, "Coordination", 0x060002c9),
    (3, false, "Quickness", 0x060002c6),
    (5, false, "Focus", 0x060002c5),
    (6, false, "Self", 0x060002c7),
    (1, true, "Health", 0x06000fa5),
    (3, true, "Stamina", 0x06000fa3),
    (5, true, "Mana", 0x06000fa4),
];
#[derive(Debug)]
pub struct Stats {
    pub skills: bool,
    selected: Option<(u32, bool)>,
    scroll: i32,
    pending: Option<(u32, bool, u32, i32)>,
}
impl Stats {
    pub fn new(skills: bool) -> Self {
        Self {
            skills,
            selected: None,
            scroll: 0,
            pending: None,
        }
    }
}
#[derive(Clone)]
enum Row<'a> {
    Heading(&'static str, u32),
    Skill(&'a SkillEntry),
}
fn skill_rows(game: &dyn GameView) -> Vec<Row<'_>> {
    let mut rows = vec![];
    for (sac, title, art) in [
        (3, "Specialized Skills", 0x06000f90),
        (2, "Trained Skills", 0x06000f86),
        (1, "Untrained Skills", 0x06000f98),
        (0, "Unusable Skills", 0x06000f89),
    ] {
        rows.push(Row::Heading(title, art));
        let mut group: Vec<_> = game
            .skills()
            .iter()
            .filter(|s| match sac {
                3 | 2 => s.sac == sac,
                1 => s.sac == 1 && s.effective > 0,
                _ => s.sac == 1 && s.effective == 0,
            })
            .collect();
        group.sort_by(|a, b| a.name.cmp(&b.name));
        rows.extend(group.into_iter().map(Row::Skill));
    }
    rows
}
fn selected_cost(model: &Stats, game: &dyn GameView) -> Option<(u32, u32, bool, i32, String)> {
    let (id, secondary) = model.selected?;
    if model.skills {
        let s = game.skills().iter().find(|s| s.id == id)?;
        let adv = game.skill_advancement(id)?;
        Some((
            id,
            adv.cost_to_raise,
            adv.sac < 2,
            s.effective,
            s.name.clone(),
        ))
    } else {
        let a = game.attribute_advancement(id, secondary)?;
        let name = ATTRIBUTES.iter().find(|r| r.0 == id && r.1 == secondary)?.2;
        Some((id, a.cost_to_raise, false, a.effective, name.into()))
    }
}
fn profile(frame: &mut PanelFrame, game: &dyn GameView) {
    image(frame, 0x060011a4, rect(0, 25, 300, 79), None, false, false);
    text(
        frame,
        rect(2, 29, 228, 15),
        game.character_name().unwrap_or("Receiving information..."),
        "15-6",
        CREAM,
        1,
        false,
        None,
    );
    let mut heritage = game.gender_heritage_display().unwrap_or_default();
    if let Some(title) = game.display_title() {
        if !heritage.is_empty() {
            heritage.push(' ');
        }
        heritage.push_str(&title);
    }
    text(
        frame,
        rect(2, 42, 228, 15),
        heritage,
        "15-6",
        CREAM,
        1,
        false,
        None,
    );
    text(
        frame,
        rect(4, 54, 228, 16),
        match game.pk_status() {
            PkStatus::Pk => "Player Killer",
            PkStatus::PkLite => "Player Killer Lite",
            PkStatus::Npk => "Non-Player Killer",
        },
        "15-6",
        CREAM,
        1,
        false,
        None,
    );
    text(
        frame,
        rect(234, 32, 64, 15),
        "character",
        "15-6",
        CREAM,
        1,
        false,
        None,
    );
    text(
        frame,
        rect(234, 46, 64, 15),
        "level",
        "15-6",
        CREAM,
        1,
        false,
        None,
    );
    text(
        frame,
        rect(4, 68, 134, 16),
        "Total experience (XP):",
        "15-6",
        CREAM,
        0,
        false,
        None,
    );
    image(frame, 0x060011a6, rect(3, 83, 228, 15), None, true, false);
    if let Some(xp) = game.experience_header() {
        text(
            frame,
            rect(234, 60, 64, 35),
            xp.level.to_string(),
            "35-16",
            CREAM,
            1,
            false,
            None,
        );
        text(
            frame,
            rect(142, 68, 90, 16),
            number(xp.total),
            "15-6",
            CREAM,
            2,
            false,
            None,
        );
        image(
            frame,
            0x060011a5,
            rect(3, 83, to_i32(228.0 * xp.meter_fill().clamp(0.0, 1.0)), 15),
            None,
            true,
            false,
        );
        text(
            frame,
            rect(129, 85, 101, 14),
            if xp.at_cap {
                "Infinity!".into()
            } else {
                number(xp.to_level)
            },
            "14-6",
            CREAM,
            2,
            false,
            None,
        );
    }
    text(
        frame,
        rect(4, 85, 121, 14),
        "XP for next level:",
        "14-6",
        CREAM,
        0,
        false,
        None,
    );
}
impl Panel for Stats {
    fn id(&self) -> &'static str {
        if self.skills {
            "skills"
        } else {
            "attributes"
        }
    }
    fn frame(&self, context: &Context<'_>) -> PanelFrame {
        let game = context.game;
        // With the stretched interface the rows list grows and the footer keeps to the bottom.
        let height = crate::panels::side_height() as i32;
        let dy = height - 362;
        let list_h = 198 + dy;
        let mut f = PanelFrame::new(300, height as u32);
        for (x, id, label, selected) in [
            (0, "attributes", "Attributes", !self.skills),
            (138, "skills", "Skills", self.skills),
        ] {
            art(
                f.button(id, rect(x, 0, 138, 25), label, true),
                if selected { 0x06000f95 } else { 0x06000f96 },
                0x06000f95,
                0x06001211,
            );
        }
        close(&mut f, 0x060011a9, 0x060011aa);
        profile(&mut f, game);
        let clip = [0, 104, if self.skills { 279 } else { 300 }, 302 + dy];
        if self.skills {
            let rows = skill_rows(game);
            let selected = rows
                .iter()
                .position(|r| matches!(r,Row::Skill(s) if self.selected==Some((s.id,false))));
            let max = (i32_from(rows.len()) * 22 - list_h).max(0);
            let offset = self.scroll.clamp(0, max);
            for (index, row) in rows.iter().enumerate() {
                let y = 104 + i32_from(index) * 22 - offset;
                if y + 20 <= 104 || y >= 302 + dy {
                    continue;
                }
                match row {
                    Row::Heading(title, did) => {
                        image(&mut f, *did, rect(0, y, 282, 20), Some(clip), false, false);
                        text(
                            &mut f,
                            rect(2, y + 2, 275, 16),
                            *title,
                            "16-7",
                            CREAM,
                            0,
                            false,
                            Some(clip),
                        );
                    }
                    Row::Skill(s) => {
                        image(
                            &mut f,
                            if selected == Some(index) {
                                0x06000f93
                            } else {
                                0x06000f94
                            },
                            rect(0, y, 282, 20),
                            Some(clip),
                            false,
                            false,
                        );
                        if let Some(icon) = s.icon {
                            image(&mut f, icon.0, rect(0, y, 20, 20), Some(clip), false, true);
                        }
                        text(
                            &mut f,
                            rect(27, y + 2, 211, 16),
                            &s.name,
                            "16-7",
                            DARK,
                            0,
                            false,
                            Some(clip),
                        );
                        text(
                            &mut f,
                            rect(242, y + 2, 34, 16),
                            s.effective.to_string(),
                            "16-7",
                            CREAM,
                            2,
                            false,
                            Some(clip),
                        );
                    }
                }
            }
            scrollbar(
                &mut f,
                "rows-scroll",
                rect(279, 104, 21, list_h),
                i32_from(rows.len()) * 22,
                list_h,
                offset,
                22,
                true,
            );
            list_hits(
                &mut f,
                "rows",
                rect(0, 104, 279, list_h),
                rows.len(),
                22,
                selected,
                offset,
            );
        } else {
            for (i, (id, secondary, name, icon)) in ATTRIBUTES.iter().enumerate() {
                let y = 104 + i32_from(i) * 22;
                image(
                    &mut f,
                    if self.selected == Some((*id, *secondary)) {
                        0x060011a1
                    } else {
                        0x060011a0
                    },
                    rect(0, y, 300, 20),
                    Some(clip),
                    false,
                    false,
                );
                image(&mut f, *icon, rect(0, y, 20, 20), Some(clip), false, true);
                text(
                    &mut f,
                    rect(27, y + 2, 198, 16),
                    *name,
                    "16-7",
                    DARK,
                    0,
                    false,
                    Some(clip),
                );
                let value = game
                    .attribute_advancement(*id, *secondary)
                    .map(|a| a.effective.to_string())
                    .unwrap_or_else(|| "???".into());
                text(
                    &mut f,
                    rect(229, y + 2, 66, 16),
                    value,
                    "16-7",
                    CREAM,
                    2,
                    false,
                    Some(clip),
                );
            }
            list_hits(
                &mut f,
                "rows",
                rect(0, 104, 300, list_h),
                9,
                22,
                ATTRIBUTES
                    .iter()
                    .position(|r| self.selected == Some((r.0, r.1))),
                0,
            );
        }
        // The footer's backdrop is taller than the footer: it is drawn at its own size and cut to
        // the panel.
        image(
            &mut f,
            0x060011a3,
            rect(0, 302 + dy, 300, 75),
            Some([0, 302 + dy, 300, height]),
            false,
            false,
        );
        let mut can_raise = false;
        if let Some((id, cost, credits, value, name)) = selected_cost(self, game) {
            // The footer: the selection's name and value; what raising it costs (not for a
            // trained skill, whose meter below shows it); then what the player has to spend.
            text(
                &mut f,
                rect(7, 304 + dy, 291, 16),
                if credits {
                    format!("{name} (Must be trained)")
                } else {
                    format!("{name} {value}")
                },
                "16-7",
                CREAM,
                0,
                false,
                None,
            );
            let trained_skill = self.skills && !credits;
            if !trained_skill {
                text(
                    &mut f,
                    rect(7, 320 + dy, 175, 16),
                    if credits {
                        "Skill credits needed to train:"
                    } else if self.selected.is_some_and(|s| s.1) {
                        "XP to raise max value:"
                    } else {
                        "XP to raise:"
                    },
                    "14-6",
                    CREAM,
                    0,
                    false,
                    None,
                );
                text(
                    &mut f,
                    rect(152, 320 + dy, 81, 16),
                    if cost == 0 && !credits {
                        "INFINITY".into()
                    } else {
                        number(cost)
                    },
                    "14-6",
                    CREAM,
                    2,
                    false,
                    None,
                );
            }
            let available = if credits {
                game.skill_credits()
            } else {
                game.available_experience()
            };
            text(
                &mut f,
                rect(7, 336 + dy, 141, 16),
                if credits {
                    "Skill credits available:"
                } else {
                    "Unassigned XP:"
                },
                "14-6",
                CREAM,
                0,
                false,
                None,
            );
            text(
                &mut f,
                rect(152, 336 + dy, 81, 16),
                number(available),
                "14-6",
                CREAM,
                2,
                false,
                None,
            );
            if self.skills && !credits {
                if let Some(adv) = game.skill_advancement(id) {
                    image(&mut f, 0x060011a6, rect(6, 321, 228, 15), None, true, false);
                    image(
                        &mut f,
                        0x06000f8a,
                        rect(6, 321, to_i32(228.0 * adv.meter_fill().clamp(0.0, 1.0)), 15),
                        None,
                        true,
                        false,
                    );
                    text(
                        &mut f,
                        rect(7, 323 + dy, 141, 12),
                        "XP cost to raise skill:",
                        "14-6",
                        CREAM,
                        0,
                        false,
                        None,
                    );
                    text(
                        &mut f,
                        rect(152, 323 + dy, 81, 12),
                        if cost == 0 {
                            "INFINITY".into()
                        } else {
                            number(cost)
                        },
                        "14-6",
                        CREAM,
                        2,
                        false,
                        None,
                    );
                }
            }
            let secondary = self.selected.is_some_and(|s| s.1);
            can_raise = cost > 0
                && available >= cost as i64
                && self.pending != Some((id, secondary, cost, value));
        } else {
            text(
                &mut f,
                rect(7, dy + if self.skills { 304 } else { 314 }, 291, 20),
                if self.skills {
                    "Select a skill to improve."
                } else {
                    "Select an attribute to improve."
                },
                "16-7",
                CREAM,
                1,
                false,
                None,
            );
            text(
                &mut f,
                rect(7, dy + if self.skills { 324 } else { 334 }, 141, 16),
                "Unassigned XP:",
                "14-6",
                CREAM,
                0,
                false,
                None,
            );
            text(
                &mut f,
                rect(152, dy + if self.skills { 324 } else { 334 }, 141, 16),
                number(game.available_experience()),
                "14-6",
                CREAM,
                2,
                false,
                None,
            );
            if self.skills {
                text(
                    &mut f,
                    rect(7, 340 + dy, 201, 16),
                    "Skill credits available:",
                    "14-6",
                    CREAM,
                    0,
                    false,
                    None,
                );
                text(
                    &mut f,
                    rect(212, 340 + dy, 81, 16),
                    number(game.skill_credits()),
                    "14-6",
                    CREAM,
                    2,
                    false,
                    None,
                );
            }
        }
        // The raise button is shown only while something is selected.
        if self.selected.is_some() {
            art(
                f.button("raise", rect(252, 322 + dy, 43, 37), "", can_raise),
                0x060011a7,
                0x0600119f,
                0x060011a2,
            );
        }
        f
    }
    fn event(&mut self, event: ControlEvent, context: &Context<'_>) -> Vec<PanelAction> {
        match event {
            ControlEvent::Activate(id) if id == "close" => return vec![PanelAction::Close],
            ControlEvent::Activate(id) if id == "attributes" || id == "skills" => {
                self.skills = id == "skills";
                self.selected = None;
                self.scroll = 0;
            }
            ControlEvent::Scroll { id, value } if id == "rows" || id == "rows-scroll" => {
                self.scroll = value.max(0)
            }
            ControlEvent::Select { id, index } if id == "rows" => {
                let next = if self.skills {
                    match skill_rows(context.game).get(index) {
                        Some(Row::Skill(s)) => Some((s.id, false)),
                        _ => None,
                    }
                } else {
                    ATTRIBUTES.get(index).map(|r| (r.0, r.1))
                };
                self.selected = if self.selected == next { None } else { next };
            }
            ControlEvent::Activate(id) if id == "raise" => {
                if let Some((id, cost, credits, value, name)) = selected_cost(self, context.game) {
                    let secondary = self.selected.unwrap().1;
                    let key = (id, secondary, cost, value);
                    let available = if credits {
                        context.game.skill_credits()
                    } else {
                        context.game.available_experience()
                    };
                    if available < cost as i64 || cost == 0 || self.pending == Some(key) {
                        return vec![];
                    }
                    if !credits {
                        self.pending = Some(key);
                    }
                    let request = if self.skills {
                        if credits {
                            UiRequest::TrainSkillAdvancementClass {
                                skill: id,
                                credits: cost,
                            }
                        } else {
                            UiRequest::TrainSkill {
                                skill: id,
                                xp: cost,
                            }
                        }
                    } else if secondary {
                        UiRequest::TrainAttribute2nd {
                            vital: id,
                            xp: cost,
                        }
                    } else {
                        UiRequest::TrainAttribute {
                            attribute: id,
                            xp: cost,
                        }
                    };
                    if credits {
                        return vec![PanelAction::Confirm {
                            id: "train-skill".into(),
                            text: format!(
                                "Spend {} credits on {}?\n\n(Default is No)",
                                number(cost),
                                name
                            ),
                            accept: vec![PanelAction::Game(request)],
                        }];
                    }
                    return vec![PanelAction::Game(request)];
                }
            }
            _ => {}
        }
        vec![]
    }
}
