//! The renderer the client starts on, as an options page chooses it: `Render.Renderer`, the same
//! `Renderer=` preference the preferences file holds and `--renderer` overrides.
//!
//! The device is made once, at start-up, so a choice made on the page applies at the next start:
//! the store keeps it, the save at exit writes it into `[Render]`, and the next start reads it as
//! it reads a `Renderer=` the player typed. Until a page chooses one the option holds no choice
//! and the save leaves it out, so a profile that never chose keeps whatever it had, or nothing.
//!
//! Only the Horizon interface shows the choice, and only where there is one to make: a page lists
//! the renderers the build can create ([`RendererStatus::offered`]), which is none in the browser.

use crate::renderer::RendererChoice;
use crate::view::PrefValue;

/// `[Render] Renderer`.
pub const RENDERER: &str = crate::options::names::RENDERER;

/// The row's caption.
pub const CAPTION: &str = "Renderer";

/// The heading over the row.
pub const HEADING: &str = "Rendering";

/// The value the option holds until a page chooses a renderer: no choice, left out of the save.
const UNCHOSEN: i32 = 0;

/// The value the store holds for `choice`.
#[must_use]
pub const fn value(choice: RendererChoice) -> i32 {
    match choice {
        RendererChoice::Vulkan => 1,
        RendererChoice::D3d12 => 2,
        RendererChoice::Wgpu => 3,
    }
}

/// The renderer a stored value names; `None` for no choice.
#[must_use]
pub const fn from_value(v: i32) -> Option<RendererChoice> {
    match v {
        1 => Some(RendererChoice::Vulkan),
        2 => Some(RendererChoice::D3d12),
        3 => Some(RendererChoice::Wgpu),
        _ => None,
    }
}

/// The renderer the store holds: the page's choice, or the `[Render] Renderer=` the profile was
/// loaded with. `None` when neither named one.
#[must_use]
pub fn stored() -> Option<RendererChoice> {
    match super::store::inq_value(RENDERER) {
        Some(PrefValue::Int(v)) => from_value(v),
        _ => None,
    }
}

/// The value a preferences-file word stands for, in the command line's spellings; `None` for
/// another name or a word it cannot read, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !name.eq_ignore_ascii_case(RENDERER) {
        return None;
    }
    RendererChoice::parse(raw).map(value)
}

/// How the save writes the value: the command line's own word. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !name.eq_ignore_ascii_case(RENDERER) {
        return None;
    }
    match v {
        PrefValue::Int(i) => from_value(*i).map(|c| c.name().to_owned()),
        _ => None,
    }
}

/// Every renderer as a choice; `None` for any other name. A page lists only those its build can
/// create, from [`RendererStatus::offered`].
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !name.eq_ignore_ascii_case(RENDERER) {
        return None;
    }
    Some(
        RendererChoice::ALL
            .iter()
            .map(|c| super::store::Choice {
                label: c.label().to_owned(),
                value: value(*c),
            })
            .collect(),
    )
}

/// Register the option in the option value store, holding no choice.
pub fn register() -> usize {
    usize::from(super::store::register_preference(
        RENDERER,
        PrefValue::Int(UNCHOSEN),
        super::store::DataType::UInt,
    ))
}

/// Whether the store's save leaves `name` out of the profile: the renderer option while it holds
/// no choice.
#[must_use]
pub fn left_out_of_save(name: &str, v: &PrefValue) -> bool {
    name.eq_ignore_ascii_case(RENDERER) && *v == PrefValue::Int(UNCHOSEN)
}

/// Which renderers a page may offer and which one is drawing, for the renderer choice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RendererStatus {
    /// The renderers this build can create, the default first. Empty where there is nothing to
    /// choose: the browser, which has one renderer, and a run with no graphics device.
    pub offered: Vec<RendererChoice>,
    /// The renderer drawing now.
    pub running: Option<RendererChoice>,
    /// The renderer the preferences file named when the client started.
    pub preference: Option<RendererChoice>,
    /// The renderer `--renderer` named on the command line, which wins over the preference for
    /// the run it is given to.
    pub command_line: Option<RendererChoice>,
}

impl RendererStatus {
    /// Whether a page shows the choice at all.
    #[must_use]
    pub fn shown(&self) -> bool {
        !self.offered.is_empty()
    }

    /// The renderer a start without `--renderer` comes up on, with `stored` the choice the store
    /// holds: that choice, else the one the preferences file named at start-up, when this build
    /// can create it; else the default. A start falls back the same way.
    #[must_use]
    pub fn next(&self, stored: Option<RendererChoice>) -> Option<RendererChoice> {
        stored
            .or(self.preference)
            .filter(|c| self.offered.contains(c))
            .or_else(|| self.offered.first().copied())
    }

    /// The renderer this start asked for: the one `--renderer` named, else the one the
    /// preferences file named. `None` when neither did, and the start took the default. A start
    /// that cannot create what it asked for comes up on another.
    #[must_use]
    pub fn asked(&self) -> Option<RendererChoice> {
        self.command_line.or(self.preference)
    }

    /// The line under the choice: the renderer in use now, and when the next start comes up on
    /// another, which. `None` when the choice is not shown.
    #[must_use]
    pub fn notice(&self, stored: Option<RendererChoice>) -> Option<String> {
        let running = self.running.filter(|_| self.shown())?;
        let next = self.next(stored)?;
        let now = running.label();
        if let Some(named) = self.command_line {
            if named != running {
                return Some(format!(
                    "Renderer: {}, which --renderer names, did not start here (now {now})",
                    named.label()
                ));
            }
            return Some(if next == running {
                format!("In use now: {now}, which --renderer on the command line names")
            } else {
                format!(
                    "Renderer: {} at a restart without --renderer (now {now}, from the command \
                     line)",
                    next.label()
                )
            });
        }
        if next == running {
            return Some(format!("In use now: {now}"));
        }
        // The start asked for what is still chosen and came up on another: it could not start.
        if self.next(None) == Some(next) {
            return Some(format!(
                "Renderer: {} did not start here (now {now})",
                next.label()
            ));
        }
        Some(format!(
            "Renderer: {} at the next restart (now {now})",
            next.label()
        ))
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own renderer choice; retail had one renderer).
    use super::*;
    use crate::options::{sheet, store};
    use crate::persist::preferences::UserPreferences;
    use RendererChoice::{D3d12, Vulkan, Wgpu};

    #[test]
    fn a_chosen_renderer_is_saved_as_the_renderer_preference_and_no_choice_saves_nothing() {
        store::init();
        assert_eq!(stored(), None);
        let saved = store::save().to_text();
        assert!(!saved.contains("Renderer"), "{saved}");
        store::set_value(RENDERER, PrefValue::Int(value(Wgpu)));
        assert_eq!(stored(), Some(Wgpu));
        let saved = store::save().to_text();
        let file = UserPreferences::parse(&saved).unwrap();
        assert_eq!(file.get("Render.Renderer"), Some("wgpu"));
        // Read back as the next start reads it.
        store::init();
        assert_eq!(store::load(&file).0, file.entries.len());
        assert_eq!(stored(), Some(Wgpu));
    }

    #[test]
    fn the_renderer_preference_reads_the_command_line_spellings_and_keeps_an_unknown_word() {
        store::init();
        let ini = UserPreferences::parse("[Render]\nRenderer=DX12\n").unwrap();
        assert_eq!(store::load(&ini), (1, 0));
        assert_eq!(stored(), Some(D3d12));
        let mut merged = UserPreferences::parse("[Render]\nRenderer=glide\n").unwrap();
        store::init();
        assert_eq!(store::load(&merged), (0, 1));
        store::save_into(&mut merged);
        assert_eq!(
            merged.get("Render.Renderer"),
            Some("glide"),
            "a word this build cannot read is left as the player wrote it"
        );
        let rows = choice_rows("render.renderer").unwrap();
        assert_eq!(
            rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            ["Vulkan", "D3D12", "wgpu"]
        );
    }

    #[test]
    fn no_page_of_the_shared_sheet_has_a_renderer_row() {
        use crate::options::interface::Interface;
        for page in [
            sheet::PageId::GameSupport,
            sheet::PageId::Character,
            sheet::PageId::Chat,
            sheet::PageId::Client,
        ] {
            for ui in Interface::ALL {
                assert!(
                    sheet::rows_for(page, ui).all(|r| !format!("{:?}", r.value).contains(RENDERER)),
                    "{page:?} {ui:?}"
                );
            }
        }
    }

    fn desktop() -> RendererStatus {
        RendererStatus {
            offered: vec![Vulkan, Wgpu],
            running: Some(Vulkan),
            preference: None,
            command_line: None,
        }
    }

    #[test]
    fn the_notice_names_the_renderer_in_use_and_the_one_the_next_start_takes() {
        let s = desktop();
        assert_eq!(s.next(None), Some(Vulkan));
        assert_eq!(s.notice(None).as_deref(), Some("In use now: Vulkan"));
        assert_eq!(
            s.notice(Some(Vulkan)).as_deref(),
            Some("In use now: Vulkan")
        );
        assert_eq!(
            s.notice(Some(Wgpu)).as_deref(),
            Some("Renderer: wgpu at the next restart (now Vulkan)")
        );
        // A renderer this build cannot create is not what the next start takes.
        assert_eq!(s.next(Some(D3d12)), Some(Vulkan));
        assert_eq!(s.notice(Some(D3d12)).as_deref(), Some("In use now: Vulkan"));
        // The preferences file named wgpu at start-up and the start came up on it.
        let on_wgpu = RendererStatus {
            running: Some(Wgpu),
            preference: Some(Wgpu),
            ..desktop()
        };
        assert_eq!(on_wgpu.notice(None).as_deref(), Some("In use now: wgpu"));
        assert_eq!(
            on_wgpu.notice(Some(Vulkan)).as_deref(),
            Some("Renderer: Vulkan at the next restart (now wgpu)")
        );
        // It named wgpu and the start fell back to the default.
        let fell_back = RendererStatus {
            preference: Some(Wgpu),
            ..desktop()
        };
        assert_eq!(
            fell_back.notice(None).as_deref(),
            Some("Renderer: wgpu did not start here (now Vulkan)")
        );
    }

    #[test]
    fn the_notice_says_when_the_command_line_wins_over_the_choice() {
        let named = RendererStatus {
            command_line: Some(Vulkan),
            preference: Some(Wgpu),
            ..desktop()
        };
        assert_eq!(
            named.notice(None).as_deref(),
            Some("Renderer: wgpu at a restart without --renderer (now Vulkan, from the command line)")
        );
        assert_eq!(
            named.notice(Some(Vulkan)).as_deref(),
            Some("In use now: Vulkan, which --renderer on the command line names")
        );
        assert_eq!(named.asked(), Some(Vulkan), "the command line wins");
        // `--renderer wgpu`, and the start could not create it.
        let fell_back = RendererStatus {
            command_line: Some(Wgpu),
            ..desktop()
        };
        assert_eq!(fell_back.asked(), Some(Wgpu));
        for stored in [None, Some(Vulkan), Some(Wgpu)] {
            assert_eq!(
                fell_back.notice(stored).as_deref(),
                Some("Renderer: wgpu, which --renderer names, did not start here (now Vulkan)"),
                "{stored:?}"
            );
        }
    }

    #[test]
    fn with_nothing_to_choose_there_is_no_choice_and_no_notice() {
        let browser = RendererStatus {
            offered: Vec::new(),
            running: Some(Wgpu),
            ..RendererStatus::default()
        };
        assert!(!browser.shown());
        assert_eq!(browser.notice(None), None);
        assert_eq!(browser.notice(Some(Vulkan)), None);
        assert_eq!(RendererStatus::default().notice(None), None);
    }
}
