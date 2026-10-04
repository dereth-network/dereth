//! The character information panel: attributes, vitals and skills, and raising or training them.
use super::super::*;
use super::common::*;
use crate::int::i32_from;
use dereth_client_contract::{PkStatus, SkillEntry};
use dereth_presentation::{stats as shared, DisplayVariant};
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
    /// On a world with titles, the window's third tab: the character's titles, and whether it is
    /// the one shown.
    titles: super::systems::Titles,
    on_titles: bool,
}
impl Stats {
    pub fn new(skills: bool) -> Self {
        Self {
            skills,
            selected: None,
            scroll: 0,
            pending: None,
            titles: super::systems::Titles::default(),
            on_titles: false,
        }
    }
}

/// Whether the character window has a Titles tab: on a world with titles.
fn titles_tab(game: &dyn GameView) -> bool {
    super::systems::era_has(game, |e| e.titles)
}
#[derive(Clone)]
enum Row<'a> {
    Heading(&'static str, u32),
    Skill(&'a SkillEntry),
}
fn skill_rows(game: &dyn GameView) -> Vec<Row<'_>> {
    let mut rows = vec![];
    for (group, entries) in shared::skill_groups(game.skills(), DisplayVariant::Classic) {
        let art = match group {
            shared::SkillGroup::Specialized => 0x06000f90,
            shared::SkillGroup::Trained => 0x06000f86,
            shared::SkillGroup::Untrained => 0x06000f98,
            shared::SkillGroup::Unusable => 0x06000f89,
        };
        rows.push(Row::Heading(group.label(), art));
        rows.extend(entries.into_iter().map(Row::Skill));
    }
    rows
}
fn selected_cost(model: &Stats, game: &dyn GameView) -> Option<(u32, u32, bool, i32, String)> {
    let (id, secondary) = model.selected?;
    let name = ATTRIBUTES
        .iter()
        .find(|r| r.0 == id && r.1 == secondary)
        .map_or("", |r| r.2);
    let selected = shared::selected_stat(game, id, secondary, model.skills, name)?;
    Some((
        selected.id,
        selected.cost,
        selected.credits,
        selected.value,
        selected.name,
    ))
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
    let header = shared::HeaderInputs::gather(game);
    let heritage = header.heritage_line_for(DisplayVariant::Classic);
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
        let height = context.layout.side_height() as i32;
        let dy = height - 362;
        let luminance = shared::HeaderInputs::gather(game).luminance_line();
        let extra = if luminance.0.is_empty() { 0 } else { 16 };
        let list_top = 104 + extra;
        let list_h = 198 + dy - extra;
        let mut f = PanelFrame::new(300, height as u32);
        // Two 138-pixel tabs, or three of 92 with the Titles tab.
        let titles = titles_tab(game);
        let width = if titles { 92 } else { 138 };
        let on_titles = titles && self.on_titles;
        for (i, (id, label, selected)) in [
            ("attributes", "Attributes", !self.skills && !on_titles),
            ("skills", "Skills", self.skills && !on_titles),
            ("titles", "Titles", on_titles),
        ]
        .into_iter()
        .enumerate()
        {
            if i == 2 && !titles {
                continue;
            }
            art(
                f.button(id, rect(i32_from(i) * width, 0, width, 25), label, true),
                if selected { 0x06000f95 } else { 0x06000f96 },
                0x06000f95,
                0x06001211,
            );
        }
        close(&mut f, 0x060011a9, 0x060011aa);
        if on_titles {
            image(
                &mut f,
                0x06001398,
                rect(0, 25, 300, height - 25),
                None,
                true,
                false,
            );
            self.titles.body(&mut f, height, context);
            return f;
        }
        profile(&mut f, game);
        if extra != 0 {
            image(&mut f, 0x06001398, rect(0, 104, 300, 16), None, true, false);
            text(
                &mut f,
                rect(4, 104, 292, 16),
                format!("{} {}", luminance.0, luminance.1),
                "14-6",
                CREAM,
                0,
                false,
                None,
            );
        }
        let clip = [
            0,
            list_top,
            if self.skills || extra != 0 { 279 } else { 300 },
            302 + dy,
        ];
        if self.skills {
            let rows = skill_rows(game);
            let selected = rows
                .iter()
                .position(|r| matches!(r,Row::Skill(s) if self.selected==Some((s.id,false))));
            let max = (i32_from(rows.len()) * 22 - list_h).max(0);
            let offset = self.scroll.clamp(0, max);
            for (index, row) in rows.iter().enumerate() {
                let y = list_top + i32_from(index) * 22 - offset;
                if y + 20 <= list_top || y >= 302 + dy {
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
                            stat_color(shared::value_font(s.effective, s.level, s.vitae)),
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
                rect(279, list_top, 21, list_h),
                i32_from(rows.len()) * 22,
                list_h,
                offset,
                22,
                true,
            );
            list_hits(
                &mut f,
                "rows",
                rect(0, list_top, 279, list_h),
                rows.len(),
                22,
                selected,
                offset,
            );
        } else {
            let offset = self.scroll.clamp(0, (198 - list_h).max(0));
            for (i, (id, secondary, name, icon)) in ATTRIBUTES.iter().enumerate() {
                let y = list_top + i32_from(i) * 22 - offset;
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
                    rect(27, y + 2, if extra == 0 { 198 } else { 181 }, 16),
                    *name,
                    "16-7",
                    DARK,
                    0,
                    false,
                    Some(clip),
                );
                let advancement = game.attribute_advancement(*id, *secondary);
                let value = advancement
                    .map(|a| a.effective.to_string())
                    .unwrap_or_else(|| "???".into());
                let color = advancement.map_or(CREAM, |a| {
                    stat_color(shared::value_font(a.effective, a.value, a.vitae))
                });
                text(
                    &mut f,
                    rect(if extra == 0 { 229 } else { 212 }, y + 2, 66, 16),
                    value,
                    "16-7",
                    color,
                    2,
                    false,
                    Some(clip),
                );
            }
            if extra != 0 {
                scrollbar(
                    &mut f,
                    "rows-scroll",
                    rect(279, list_top, 21, list_h),
                    198,
                    list_h,
                    offset,
                    22,
                    true,
                );
            }
            list_hits(
                &mut f,
                "rows",
                rect(0, list_top, if extra == 0 { 300 } else { 279 }, list_h),
                9,
                22,
                ATTRIBUTES
                    .iter()
                    .position(|r| self.selected == Some((r.0, r.1))),
                offset,
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
                shared::stat_title(&name, value, credits, DisplayVariant::Classic),
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
            can_raise = shared::can_raise(u64::from(cost), u64::try_from(available).unwrap_or(0))
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
            ControlEvent::Activate(id) if id == "titles" && titles_tab(context.game) => {
                self.on_titles = true;
            }
            ControlEvent::Activate(id) if id == "attributes" || id == "skills" => {
                self.on_titles = false;
                self.skills = id == "skills";
                self.selected = None;
                self.scroll = 0;
            }
            event if self.on_titles && titles_tab(context.game) => {
                return self.titles.body_event(event, context);
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
                    if !shared::can_raise(u64::from(cost), u64::try_from(available).unwrap_or(0))
                        || self.pending == Some(key)
                    {
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

fn stat_color(index: u32) -> u32 {
    match index {
        1 => 0xff00ff00,
        2 => 0xffff0000,
        _ => CREAM,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_client_contract::statmgmt::XpHeader;
    use dereth_client_contract::{AttributeAdvancement, SkillAdvancement, VitaeDisplay};
    #[derive(Debug)]
    struct View {
        skills: Vec<SkillEntry>,
        vitae: Option<VitaeDisplay>,
    }
    impl GameView for View {
        fn skills(&self) -> &[SkillEntry] {
            &self.skills
        }
        fn skill_advancement(&self, _: u32) -> Option<SkillAdvancement> {
            Some(SkillAdvancement {
                sac: 2,
                cost_to_raise: 10,
                ..Default::default()
            })
        }
        fn attribute_advancement(&self, _: u32, _: bool) -> Option<AttributeAdvancement> {
            Some(AttributeAdvancement {
                effective: 98,
                value: 100,
                vitae: -5,
                ..Default::default()
            })
        }
        fn vitae_display(&self) -> Option<VitaeDisplay> {
            self.vitae
        }
        fn experience_header(&self) -> Option<XpHeader> {
            Some(XpHeader {
                level: 200,
                ..Default::default()
            })
        }
        fn luminance(&self) -> (i64, i64) {
            (1234, 9000)
        }
    }
    fn context<T>(view: &dyn GameView, f: impl FnOnce(&Context<'_>) -> T) -> T {
        f(&Context {
            resources: &crate::resources::Resources::default(),
            layout: crate::panels::Layout::default(),
            now: dereth_primitives::LocalTime(0.0),
            game: view,
            pregame: &Default::default(),
            keyboard: &Default::default(),
            settings: &Default::default(),
            map_teleport_allowed: false,
            classic: &Default::default(),
        })
    }
    /// Behaviour: stats.presentation.preserves-display-variants-and-fractional-vitae
    #[test]
    fn actual_classic_rows_color_effective_values_and_project_later_luminance() {
        let view = View {
            skills: vec![SkillEntry {
                id: 1,
                name: "Buff".into(),
                icon: None,
                min_level: 0,
                sac: 2,
                level: 100,
                effective: 98,
                vitae: -5,
            }],
            vitae: None,
        };
        let frame = context(&view, |c| Stats::new(true).frame(c));
        assert!(frame.screen.commands.iter().any(
            |c| matches!(c,crate::Command::TextBox{text,color,..} if text=="98" && *color==0xff00ff00)
        ));
        assert!(frame.screen.commands.iter().any(
            |c| matches!(c,crate::Command::TextBox{text,..} if text=="Luminance: 1,234 / 9,000")
        ));
        let snapshot = dereth_client_contract::snapshot::GameSnapshot::from_view(&view);
        let copied = context(&snapshot, |c| Stats::new(true).frame(c));
        let values = |f: PanelFrame| {
            f.screen
                .commands
                .into_iter()
                .filter_map(|c| match c {
                    crate::Command::TextBox { text, color, .. } => Some((text, color)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(values(frame), values(copied));
    }
    /// Behaviour: stats.presentation.preserves-display-variants-and-fractional-vitae
    #[test]
    fn actual_classic_vitae_frame_and_tick_keep_small_positive_losses_open() {
        for (multiplier, penalty, close) in [
            (0.999, 1, false),
            (0.986, 1, false),
            (0.984, 2, false),
            (1.0, 0, true),
        ] {
            let view = View {
                skills: vec![],
                vitae: Some(VitaeDisplay {
                    multiplier,
                    threshold: 10,
                    cp_pool: 9,
                }),
            };
            let mut panel = super::super::magic::Vitae;
            context(&view, |c| {
                let frame = panel.frame(c);
                assert_eq!(frame.screen.commands.iter().any(|cmd|matches!(cmd,crate::Command::TextBox{text,..} if text.contains(&format!("lost {penalty}%")))),!close);
                assert_eq!(
                    matches!(
                        panel.event(ControlEvent::Tick, c).as_slice(),
                        [PanelAction::Close]
                    ),
                    close
                );
            });
        }
        let mut panel = super::super::magic::Vitae;
        context(
            &View {
                skills: vec![],
                vitae: None,
            },
            |c| {
                assert!(matches!(
                    panel.event(ControlEvent::Tick, c).as_slice(),
                    [PanelAction::Close]
                ))
            },
        );
    }
}
