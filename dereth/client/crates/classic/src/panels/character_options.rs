//! The classic Character settings model as an embeddable options page.
use super::*;
use crate::int::i32_from;
use crate::screens::{max_scroll, rows, OptionsModel};
#[derive(Debug, Default)]
pub struct CharacterOptions {
    model: OptionsModel,
    scroll: i32,
    loaded: bool,
}
impl CharacterOptions {
    pub fn new() -> Self {
        Self::default()
    }
}
impl Panel for CharacterOptions {
    fn id(&self) -> &'static str {
        "character-options"
    }
    fn frame(&self, context: &Context<'_>) -> PanelFrame {
        let height = crate::panels::side_height() - 25;
        let mut f = PanelFrame::new(300, height);
        crate::panels::sub_page_background(&mut f, height as i32);
        let features = context.game.era().map(|e| e.features());
        for command in self
            .model
            .render_for(self.scroll, features.as_ref())
            .commands
        {
            let include = match &command {
                Command::Image { did, y, .. } => {
                    *y >= 25
                        && !matches!(
                            did.as_str(),
                            "0600128B"
                                | "0600128D"
                                | "06001263"
                                | "0600128A"
                                | "060012BB"
                                | "060012BC"
                                | "060012BD"
                        )
                        && !(*y == 322)
                }
                Command::Text { text, y, .. } => {
                    *y >= 25 && !matches!(text.as_str(), "Apply" | "Reset" | "Defaults")
                }
                _ => true,
            };
            if include {
                f.screen
                    .commands
                    .push(crate::desktop::translate_command(command, 0, -25));
            }
        }
        let viewport = rect(4, 16, 280, 264);
        for (i, row) in rows().iter().enumerate() {
            if let Some(checked) = self.model.checked(i) {
                let area = rect(14, 16 + row.y - self.scroll, 13, 13);
                if let Some(hit) = area.intersect(viewport) {
                    // A row for what the world's era lacks is greyed and takes no click.
                    let enabled = row.needs.met(features.as_ref());
                    f.check(format!("option{i}"), hit, "", checked, enabled);
                }
            }
        }
        f.control(
            "scroll",
            rect(284, 16, 16, 264),
            ControlKind::ScrollBar {
                min: 0,
                max: max_scroll(),
                value: self.scroll,
                page: 264,
                step: 20,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
        for (i, (id, title)) in [
            ("apply", "Apply"),
            ("reset", "Reset"),
            ("defaults", "Defaults"),
        ]
        .iter()
        .enumerate()
        {
            let c = f.button(
                *id,
                rect(25 + 85 * i32_from(i), 297, 80, 36),
                *title,
                self.model.button_enabled(i),
            );
            c.images = Some(["06001207", "06001208", "0600120A"].map(String::from));
            c.endcaps = Some(["06001206", "06001209", "06001205"].map(String::from));
        }
        f
    }
    fn event(&mut self, event: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        if !self.loaded {
            self.model = OptionsModel::from_words(c.classic.option_words);
            self.model
                .load_timestamp_format(c.classic.timestamp_format.clone());
            self.loaded = true;
        }
        match event {
            ControlEvent::Check { id, checked } => {
                if let Some(i) = id.strip_prefix("option").and_then(|s| s.parse().ok()) {
                    self.model.set_checked(i, checked);
                }
            }
            ControlEvent::Scroll { id, value } if id == "scroll" => {
                self.scroll = value.clamp(0, max_scroll())
            }
            ControlEvent::Activate(id) => match id.as_str() {
                "apply" => {
                    let a = self.model.apply();
                    return vec![PanelAction::Host(HostAction::CharacterOptions {
                        words: a.words,
                        timestamp_format: a.timestamp_format,
                        save: true,
                    })];
                }
                "reset" => self.model.reset(),
                "defaults" => {
                    self.model.defaults();
                    return vec![PanelAction::Host(HostAction::CharacterOptions {
                        words: self.model.applied_words(),
                        timestamp_format: self.model.timestamp_format().into(),
                        save: false,
                    })];
                }
                "close" => return vec![PanelAction::Close],
                _ => {}
            },
            _ => {}
        }
        vec![]
    }
}
