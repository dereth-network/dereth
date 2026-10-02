//! The combat panel: its controls, its sizing, and what pressing and releasing them sends.
use super::*;
use dereth_client_contract::view::PlayerOption;
use dereth_primitives::num::to_i32;

#[derive(Debug)]
pub(super) struct Combat {
    width: u32,
    held: Option<u32>,
}
impl Default for Combat {
    fn default() -> Self {
        Self {
            width: 491,
            held: None,
        }
    }
}
impl Panel for Combat {
    fn id(&self) -> &'static str {
        "combat"
    }
    fn resize(&mut self, width: u32, _: u32) {
        self.width = width.max(240);
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let w = self.width as i32;
        let mut f = PanelFrame::new(self.width, 64);
        image(&mut f, 0x06001920, rect(0, 0, 10, 64), true, None);
        image(&mut f, 0x06001921, rect(10, 0, w - 110, 64), true, None);
        image(&mut f, 0x06001922, rect(w - 100, 0, 100, 64), false, None);
        let bar = c.game.combat_bar();
        for (height, text, y) in [(1, "High", 6), (2, "Medium", 25), (3, "Low", 44)] {
            let selected = bar.requested_attack_height == height;
            let control = f.button(
                format!("attack:{height}"),
                rect(w - 78, y, 72, 19),
                text,
                true,
            );
            control.images = Some(
                if selected {
                    ["0600191D", "0600191C", "0600191D"]
                } else {
                    ["0600191E", "0600191B", "0600191E"]
                }
                .map(String::from),
            );
            control.capture_edges = true;
            control.font = "15-6".into();
        }
        for (id, caption, x, option) in [
            (
                "repeat",
                "Repeat Attacks",
                6,
                PlayerOption::AutoRepeatAttack,
            ),
            ("auto-target", "Auto Target", 120, PlayerOption::AutoTarget),
        ] {
            let checked = c.game.player_option(option);
            image(
                &mut f,
                if checked { 0x0600191a } else { 0x0600191f },
                rect(x, 44, 17, 17),
                false,
                None,
            );
            f.check(id, rect(x, 44, 17, 17), "", checked, true).paint = false;
            f.text_box(
                rect(x + 17, 44, 90, 17),
                caption,
                "15-6",
                0xffffffff,
                TextAlign::Left,
                false,
                None,
            );
        }
        f.text_box(
            rect(9, 25, 60, 15),
            "Speed",
            "15-6",
            0xffffffff,
            TextAlign::Left,
            false,
            None,
        );
        f.text_box(
            rect(w - 146, 25, 60, 15),
            if c.game.combat_mode() == 4 {
                "Accuracy"
            } else {
                "Power"
            },
            "15-6",
            0xffffffff,
            TextAlign::Right,
            false,
            None,
        );
        let meter = rect(9, 10, w - 95, 13);
        image(&mut f, 0x06001919, meter, true, None);
        let level = c
            .classic
            .classic_power_level
            .unwrap_or(if bar.power_bar_mode == 1 {
                bar.level
            } else {
                0.0
            })
            .clamp(0.0, 1.0);
        image(
            &mut f,
            0x06001200,
            meter,
            true,
            Some(rect(
                meter.x,
                meter.y,
                to_i32(meter.w as f32 * level),
                meter.h,
            )),
        );
        let desired = bar.desired_power.clamp(0.0, 1.0);
        image(
            &mut f,
            0x06001923,
            rect(
                meter.x + to_i32((meter.w - 11) as f32 * desired),
                meter.y,
                12,
                14,
            ),
            false,
            None,
        );
        f.control(
            "power",
            meter,
            ControlKind::ScrollBar {
                min: 0,
                max: 1000,
                value: to_i32(desired * 1000.0),
                page: 0,
                step: 1,
                vertical: false,
                arrow_size: 0,
                thumb_size: 12,
            },
            true,
        )
        .paint = false;
        f
    }
    fn event(&mut self, e: ControlEvent, _: &Context<'_>) -> Vec<PanelAction> {
        match e {
            ControlEvent::Held { id, pressed } => {
                let Some(height) = id
                    .strip_prefix("attack:")
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|h| (1..=3).contains(h))
                else {
                    return vec![];
                };
                if pressed && self.held != Some(height) {
                    self.held = Some(height);
                    vec![PanelAction::Game(UiRequest::CombatSetAttackHeight {
                        height,
                    })]
                } else if !pressed && self.held == Some(height) {
                    self.held = None;
                    vec![PanelAction::Game(UiRequest::CombatEndAttack { height })]
                } else {
                    vec![]
                }
            }
            ControlEvent::Scroll { id, value } if id == "power" => {
                vec![PanelAction::Game(UiRequest::CombatSetDesiredPower {
                    position: value.clamp(0, 1000) as u32,
                })]
            }
            ControlEvent::Check { id, checked } => {
                let option = match id.as_str() {
                    "repeat" => PlayerOption::AutoRepeatAttack,
                    "auto-target" => PlayerOption::AutoTarget,
                    _ => return vec![],
                };
                vec![
                    PanelAction::Game(UiRequest::SetPlayerOption(option, checked)),
                    PanelAction::Game(UiRequest::SavePlayerOptions),
                ]
            }
            ControlEvent::Activate(id) if id == "close" => {
                self.held.take().map_or_else(Vec::new, |height| {
                    vec![PanelAction::Game(UiRequest::CombatEndAttack { height })]
                })
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic panel adapter; no retail behaviour claim).
    use super::*;
    #[derive(Debug, Default)]
    struct World;
    impl GameView for World {}
    #[test]
    fn held_attack_emits_one_request_per_edge_and_never_on_plain_activation() {
        let mut p = Combat::default();
        let c = Context {
            game: &World,
            pregame: &PregameView::default(),
            keyboard: &KeyboardState::default(),
            settings: &ClassicSettings::default(),
            map_teleport_allowed: false,
            classic: &ClassicState::default(),
        };
        assert_eq!(
            p.event(
                ControlEvent::Held {
                    id: "attack:1".into(),
                    pressed: true
                },
                &c
            ),
            vec![PanelAction::Game(UiRequest::CombatSetAttackHeight {
                height: 1
            })]
        );
        assert_eq!(
            p.event(
                ControlEvent::Held {
                    id: "attack:1".into(),
                    pressed: false
                },
                &c
            ),
            vec![PanelAction::Game(UiRequest::CombatEndAttack { height: 1 })]
        );
        assert!(p
            .event(ControlEvent::Activate("attack:1".into()), &c)
            .is_empty());
        assert!(p
            .event(
                ControlEvent::Held {
                    id: "attack:1".into(),
                    pressed: false
                },
                &c
            )
            .is_empty());
    }
}
