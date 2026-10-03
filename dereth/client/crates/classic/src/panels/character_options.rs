//! The classic Character settings model as an embeddable options page.
use super::*;
use crate::int::i32_from;
use crate::screens::{rows, OptionsModel, ROW_HEIGHT};
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
/// The rows the page shows for a world whose era has `features`: every heading, and every option
/// but those for what the era lacks, which the page leaves out. Each is the row's index among
/// [`rows`] and where it sits down the list.
fn shown(features: Option<&dereth_primitives::EraFeatures>) -> Vec<(usize, i32)> {
    rows()
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == "heading" || r.needs.met(features))
        .enumerate()
        .map(|(k, (i, _))| (i, 6 + ROW_HEIGHT * i32_from(k)))
        .collect()
}

/// How tall the list is with `count` rows.
fn content(count: usize) -> i32 {
    i32_from(count) * ROW_HEIGHT + 6
}

impl Panel for CharacterOptions {
    fn id(&self) -> &'static str {
        "character-options"
    }
    fn frame(&self, context: &Context<'_>) -> PanelFrame {
        let height = crate::panels::side_height() - 25;
        let mut f = PanelFrame::new(300, height);
        crate::panels::sub_page_background(&mut f, height as i32);
        let page = crate::panels::OptionsPage::current();
        page.background(&mut f);
        let features = Some(context.game.era_features());
        let shown = shown(features.as_ref());
        let scroll = self.scroll.clamp(0, page.max_scroll(content(shown.len())));
        let clip = page.clip();
        for (i, y) in shown.iter().copied() {
            let row = &rows()[i];
            let y = page.view.y + y - scroll;
            if y + ROW_HEIGHT <= page.view.y || y >= page.view.y + page.view.h {
                continue;
            }
            let heading = row.kind == "heading";
            f.label(
                if heading { 16 } else { 36 },
                y + 2,
                if row.option == Some(dereth_client_contract::PlayerOption::FellowshipShareXP) {
                    dereth_client_contract::era::fellowship_share_caption(
                        context.game.era_features(),
                    )
                    .fallback
                    .into()
                } else {
                    row.caption.clone()
                },
                if heading { "courier-14-7" } else { "15-6" },
                if heading { 0xff00_c8e1 } else { 0xffd2_d2c8 },
                Some(clip),
            );
            if let Some(checked) = self.model.checked(i) {
                if let Some(hit) = rect(14, y, 13, 13).intersect(page.view) {
                    f.check(format!("option{i}"), hit, "", checked, true);
                }
            }
        }
        page.scroll_bar(&mut f, "scroll", content(shown.len()), scroll, ROW_HEIGHT);
        for (i, (id, title)) in [
            ("apply", "Apply"),
            ("reset", "Reset"),
            ("defaults", "Defaults"),
        ]
        .iter()
        .enumerate()
        {
            page.button(&mut f, i32_from(i), id, title, self.model.button_enabled(i));
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
                let features = Some(c.game.era_features());
                let page = crate::panels::OptionsPage::current();
                self.scroll =
                    value.clamp(0, page.max_scroll(content(shown(features.as_ref()).len())));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: options.pages.a-row-for-what-the-worlds-era-lacks-is-not-shown
    #[test]
    fn the_classic_page_leaves_out_the_rows_for_what_the_worlds_era_lacks() {
        let everything = shown(None);
        assert_eq!(everything.len(), rows().len());
        let early = dereth_primitives::EraId::Infiltration.features();
        let fewer = shown(Some(&early));
        let lacking = rows().iter().filter(|r| !r.needs.met(Some(&early))).count();
        assert!(lacking > 0);
        assert_eq!(fewer.len(), rows().len() - lacking);
        assert!(fewer
            .iter()
            .all(|(i, _)| rows()[*i].needs.met(Some(&early))));
        // The rows that stay close up, a row apart.
        for (k, (_, y)) in fewer.iter().enumerate() {
            assert_eq!(*y, 6 + ROW_HEIGHT * i32_from(k));
        }
    }
}
