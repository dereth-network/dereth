//! The classic options window: the shared options set's four pages (Game and Support, Character
//! Options, Chat Options and Client Options), drawn the classic interface's way.
//!
//! The Client Options page scrolls, as the Character page does. Its rows that the classic
//! interface always had (sound, the window size, brightness, camera stiffness, the graphics
//! performance, the texture sizes) keep their own steps and go through the settings host as
//! before; the rest are edited as the preferences they are, and Apply writes them.
use super::*;
use dereth_client_contract::options::sheet::{self, Face, PageId, Row, Value};
use dereth_client_contract::options::{classic, store};
use dereth_client_contract::PrefValue;
use dereth_primitives::num::to_i32;

pub fn make(id: &str) -> Option<Box<dyn Panel>> {
    Some(Box::new(Settings {
        tab: if id == "sound-graphics" { 2 } else { 0 },
        draft: None,
        saved: None,
        dirty: false,
        scroll: 0,
        chat_scroll: 0,
        own: Vec::new(),
        character: super::super::character_options::CharacterOptions::new(),
    }))
}

#[derive(Debug)]
struct Settings {
    /// 0 Game and Support, 1 Character Options, 2 Client Options, 3 Chat Options.
    tab: usize,
    draft: Option<ClassicSettings>,
    saved: Option<ClassicSettings>,
    dirty: bool,
    /// How far the Client Options page is scrolled.
    scroll: i32,
    /// How far the Chat Options page is scrolled.
    chat_scroll: i32,
    /// The Client Options rows edited as preferences, changed since the page was applied.
    own: Vec<(&'static str, PrefValue)>,
    character: super::super::character_options::CharacterOptions,
}

/// The page's background, the side panel's height less the tabs.
fn background() -> PanelFrame {
    let height = crate::panels::side_height() - 25;
    let mut f = PanelFrame::new(300, height);
    crate::panels::sub_page_background(&mut f, height as i32);
    f
}
fn separator(f: &mut PanelFrame, y: i32) {
    f.image("060012C5", rect(4, y, 275, 8), true, false);
    f.image("060012C4", rect(279, y, 17, 8), false, false);
}

/// How far apart the rows are: the Client page's sliders and drop-downs a few pixels further
/// than the check boxes of the others, so that no two touch.
fn row_height(page: PageId) -> i32 {
    if page == PageId::Client {
        24
    } else {
        20
    }
}
/// The caption colour of a row that cannot be changed now.
const GREY: u32 = 0xff64_6464;
const HEADING: u32 = 0xff00_c8e1;
const SLIDER_ART: [&str; 3] = ["06001285", "06001286", "06001286"];

/// One line of a scrolling page: a heading or a row.
#[derive(Clone, Copy)]
enum Line {
    Heading(&'static str),
    Row(&'static Row),
}

/// The lines of `page` as the classic interface shows it in the world `c` is in: a row for what
/// the world's era lacks is left out.
fn lines(page: PageId, c: &Context<'_>) -> Vec<Line> {
    let features = c.game.era().map(|e| e.features());
    let mut out = Vec::new();
    for (h, rows) in sheet::headings_for(page, Face::Classic) {
        out.push(Line::Heading(h.title));
        out.extend(
            rows.into_iter()
                .filter(|r| {
                    r.needs.met(features.as_ref()) && r.preference().is_none_or(|p| era_has(c, p))
                })
                .map(Line::Row),
        );
    }
    out
}

/// How tall `page`'s list is.
fn content(page: PageId, c: &Context<'_>) -> i32 {
    i32::try_from(lines(page, c).len()).unwrap_or(0) * row_height(page) + 6
}

fn max_scroll(page: PageId, c: &Context<'_>) -> i32 {
    crate::panels::OptionsPage::current().max_scroll(content(page, c))
}

/// The rows the classic interface keeps its own steps for, through the settings host.
fn hosted(preference: &str) -> bool {
    matches!(
        preference,
        "Sound.SoundFeatures"
            | "Sound.SoundDisabled"
            | "Sound.AmbientSoundDisabled"
            | "Sound.InterfaceSoundDisabled"
            | "Sound.SoundVolume"
            | "Sound.AmbientSoundVolume"
            | "Display.Resolution"
            | "Display.FullScreen"
            | "Render.ScreenBrightness"
            | "Camera.Stiffness"
            | "Render.GraphicsPerformance"
            | "Render.AutomaticDegrades"
            | "Render.BuildingDetailTextures"
            | "Render.LandscapeTextureDetail"
            | "Render.EnvironmentTextureDetail"
    )
}

/// The page's step of a texture-detail preference's value: 1 is the first, 4 the last.
fn texture_level(value: i32) -> u8 {
    u8::try_from((value - 1).clamp(0, 3)).unwrap_or(0)
}

/// A preference's slider range, from its registration.
fn range(preference: &str) -> (f32, f32) {
    dereth_client_contract::options::preferences::UI_PREFERENCES
        .iter()
        .find(|p| p.name == preference)
        .and_then(|p| p.range)
        .unwrap_or((0.0, 1.0))
}

/// The classic steps of the two texture sizes: the landscape's Full to an eighth (1 to 4), and
/// every other image's with the final client's Very High (0) before them.
const LANDSCAPE_STEPS: [&str; 4] = ["Full", "1/2", "1/4", "1/8"];
const ENVIRONMENT_STEPS: [&str; 5] = ["Very High", "Full", "1/2", "1/4", "1/8"];

/// Whether the world's era has what a row sets: the social window's Secure Trade page needs
/// trade, and its row is not shown without it.
fn era_has(c: &Context<'_>, preference: &str) -> bool {
    preference != classic::SHOW_TRADE_TAB || c.game.era().is_none_or(|e| e.features().trade)
}

/// The row of the classic Client Options page that edits `preference`.
fn client_row(preference: &str) -> Option<&'static Row> {
    sheet::rows_for(PageId::Client, Face::Classic).find(|r| {
        r.preference() == Some(preference)
            || matches!(r.value, Value::Sound { volume, .. } if volume == preference)
    })
}

impl Settings {
    fn general(&self, _c: &Context<'_>) -> PanelFrame {
        let mut f = background();
        let height = i32::try_from(crate::panels::side_height()).unwrap_or(362) - 25;
        separator(&mut f, height - 34);
        let actions = sheet::rows_for(PageId::GameSupport, Face::Classic)
            .filter(|r| !matches!(r.value, Value::Action(sheet::Act::MouseTurningSettings)));
        for (k, r) in actions.enumerate() {
            let Value::Action(act) = r.value else {
                continue;
            };
            let id = match act {
                sheet::Act::ExitToCharacterSelection => "leave",
                sheet::Act::ExitGame => "exit",
                sheet::Act::ConfigureKeyboard => "keyboard",
                sheet::Act::UrgentAssistance => "urgent",
                sheet::Act::ReportAbuse => "abuse",
                sheet::Act::MouseTurningSettings => continue,
            };
            let y = 8 + 42 * i32::try_from(k).unwrap_or(0);
            f.button(id, rect(30, y, 240, 34), r.caption_for(Face::Classic), true);
        }
        centered(
            &mut f,
            rect(0, height - 23, 300, 18),
            concat!("Version ", env!("CARGO_PKG_VERSION")),
            "15-6",
        );
        f
    }

    /// A preference's value on the page: the draft's where the row is the settings host's, else
    /// what the player changed since Apply, else the store's, else the row's default.
    fn own_value(&self, preference: &'static str) -> Option<PrefValue> {
        if let Some((_, v)) = self.own.iter().find(|(n, _)| *n == preference) {
            return Some(v.clone());
        }
        store::inq_value(preference).or_else(|| {
            client_row(preference)
                .and_then(|r| r.default)
                .map(PrefValue::from)
        })
    }
    fn set_own(&mut self, preference: &'static str, value: PrefValue) {
        self.own.retain(|(n, _)| *n != preference);
        self.own.push((preference, value));
        self.dirty = true;
    }
    fn own_bool(&self, preference: &'static str) -> bool {
        matches!(self.own_value(preference), Some(PrefValue::Bool(true)))
    }
    fn own_slider(&self, preference: &'static str) -> i32 {
        let (lo, hi) = range(preference);
        let v = match self.own_value(preference) {
            Some(PrefValue::Float(v)) => v,
            _ => lo,
        };
        if hi > lo {
            to_i32(((v - lo) / (hi - lo) * 100.0).round().clamp(0.0, 100.0))
        } else {
            0
        }
    }

    /// The check box of a check row: its state and whether it can be changed.
    fn check_state(&self, s: &ClassicSettings, preference: &'static str) -> (bool, bool) {
        match preference {
            "Display.FullScreen" => (s.full_screen, true),
            "Render.AutomaticDegrades" => (s.auto_degrade, true),
            "Render.BuildingDetailTextures" => (s.environment_detail, s.detail_available),
            "Render.LandscapeDetailTextures" => (self.own_bool(preference), s.detail_available),
            _ => (self.own_bool(preference), true),
        }
    }
    /// A sound row: on, its volume (0 to 100) and whether the machine has sound.
    fn sound_state(
        &self,
        s: &ClassicSettings,
        on: &'static str,
        volume: &'static str,
    ) -> (bool, i32) {
        let pct = |v: f32| to_i32((v * 100.0).round());
        match on {
            "Sound.SoundDisabled" => (s.effects, pct(s.effects_volume)),
            "Sound.AmbientSoundDisabled" => (s.ambient, pct(s.ambient_volume)),
            "Sound.InterfaceSoundDisabled" => (s.interface, self.own_slider(volume)),
            _ => (self.own_bool(on), self.own_slider(volume)),
        }
    }
    /// A slider row's position, 0 to 100, in the classic page's direction.
    fn slider_state(&self, s: &ClassicSettings, preference: &'static str) -> i32 {
        let pct = |v: f32| to_i32((v * 100.0).round());
        match preference {
            "Render.ScreenBrightness" => pct(s.brightness),
            "Camera.Stiffness" => pct(s.camera_stiffness),
            "Render.GraphicsPerformance" => pct(s.performance),
            _ => self.own_slider(preference),
        }
    }
    /// A drop-down row's choices and the chosen one.
    fn menu_state(&self, s: &ClassicSettings, preference: &'static str) -> (Vec<String>, usize) {
        match preference {
            "Sound.SoundFeatures" => (vec!["Stereo".into(), "Mono".into()], usize::from(!s.stereo)),
            "Display.Resolution" => (
                s.resolutions
                    .iter()
                    .map(|(w, h)| format!("{w} x {h}"))
                    .collect(),
                s.resolution,
            ),
            "Render.LandscapeTextureDetail" => (
                LANDSCAPE_STEPS.map(String::from).to_vec(),
                usize::from(s.texture_levels[0].min(3)),
            ),
            "Render.EnvironmentTextureDetail" => (
                ENVIRONMENT_STEPS.map(String::from).to_vec(),
                if s.environment_very_high {
                    0
                } else {
                    usize::from(s.texture_levels[2].min(3)) + 1
                },
            ),
            _ => {
                let choices = store::choice_rows(preference).unwrap_or_default();
                let now = match self.own_value(preference) {
                    Some(PrefValue::Int(v)) => v,
                    _ => 0,
                };
                let chosen = choices.iter().position(|c| c.value == now).unwrap_or(0);
                (choices.into_iter().map(|c| c.label).collect(), chosen)
            }
        }
    }

    /// The Client Options page: the shared set's headings and the classic interface's rows under
    /// them, scrolled.
    fn client(&self, c: &Context<'_>) -> PanelFrame {
        // Unchanged, the page shows the settings as they are now, whichever interface set them.
        let s = self
            .draft
            .as_ref()
            .filter(|_| self.dirty)
            .unwrap_or(c.settings);
        let mut f = background();
        let page = crate::panels::OptionsPage::current();
        page.background(&mut f);
        let clip = Some(page.clip());
        let row = row_height(PageId::Client);
        let scroll = self.scroll.clamp(0, max_scroll(PageId::Client, c));
        for (k, line) in lines(PageId::Client, c).into_iter().enumerate() {
            let y = page.view.y + 6 + row * i32::try_from(k).unwrap_or(0) - scroll;
            if !page.shows(y - 1, 18) {
                continue;
            }
            let row = match line {
                Line::Heading(title) => {
                    f.label(16, y + 2, title, "courier-14-7", HEADING, clip);
                    continue;
                }
                Line::Row(r) => r,
            };
            let caption = row.caption_for(Face::Classic);
            match row.value {
                Value::Check(p) => {
                    let (on, enabled) = self.check_state(s, p);
                    f.check(format!("row:{p}"), rect(14, y, 13, 13), "", on, enabled);
                    f.label(
                        36,
                        y + 2,
                        caption,
                        "15-6",
                        if enabled { INK } else { GREY },
                        clip,
                    );
                }
                Value::Sound { on, volume } => {
                    let (lit, level) = self.sound_state(s, on, volume);
                    f.check(
                        format!("row:{on}"),
                        rect(14, y, 13, 13),
                        "",
                        lit,
                        s.sound_available,
                    );
                    f.label(36, y + 2, caption, "15-6", INK, clip);
                    let slider = f.slider(
                        format!("vol:{volume}"),
                        rect(160, y + 1, 115, 12),
                        0,
                        100,
                        level,
                        1,
                    );
                    slider.enabled = s.sound_available && lit;
                    slider.images = Some(SLIDER_ART.map(String::from));
                }
                Value::Slider(p) => {
                    // The bias by hand is used only while Adaptive Degrade is off.
                    let enabled = p != "Render.GraphicsPerformance" || !s.auto_degrade;
                    f.label(
                        14,
                        y + 2,
                        caption,
                        "15-6",
                        if enabled { INK } else { GREY },
                        clip,
                    );
                    let slider = f.slider(
                        format!("slider:{p}"),
                        rect(160, y + 1, 115, 12),
                        0,
                        100,
                        self.slider_state(s, p),
                        1,
                    );
                    slider.enabled = enabled;
                    slider.images = Some(SLIDER_ART.map(String::from));
                }
                Value::Menu(p) => {
                    f.label(14, y + 2, caption, "15-6", INK, clip);
                    let (options, selected) = self.menu_state(s, p);
                    let enabled = p != "Sound.SoundFeatures" || (s.sound_available && s.effects);
                    let menu = f.control(
                        format!("row:{p}"),
                        rect(160, y - 1, 120, 18),
                        ControlKind::Choice { options, selected },
                        enabled,
                    );
                    menu.list_skin = Some(crate::panels::ListSkin::OPTIONS);
                }
                _ => {}
            }
        }
        page.scroll_bar(&mut f, "scroll", content(PageId::Client, c), scroll, row);
        for (slot, (id, text)) in [
            ("apply", "Apply"),
            ("reset", "Reset"),
            ("defaults", "Defaults"),
        ]
        .into_iter()
        .enumerate()
        {
            page.button(
                &mut f,
                i32::try_from(slot).unwrap_or(0),
                id,
                text,
                self.dirty || id == "defaults",
            );
        }
        f
    }

    /// The main chat window's filter: the one the game holds, or the window's first.
    fn chat_filter(c: &Context<'_>) -> u64 {
        c.game
            .chat_window_filter(sheet::window::MAIN)
            .unwrap_or(sheet::MAIN_WINDOW_DEFAULT_FILTER)
    }

    /// The Chat Options page: which messages the chat window shows, by group.
    fn chat(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = background();
        let page = crate::panels::OptionsPage::current();
        page.background(&mut f);
        let clip = Some(page.clip());
        let filter = Self::chat_filter(c);
        let row = row_height(PageId::Chat);
        let scroll = self.chat_scroll.clamp(0, max_scroll(PageId::Chat, c));
        for (k, line) in lines(PageId::Chat, c).into_iter().enumerate() {
            let y = page.view.y + 6 + row * i32::try_from(k).unwrap_or(0) - scroll;
            if !page.shows(y, 13) {
                continue;
            }
            match line {
                Line::Heading(title) => f.label(16, y + 2, title, "courier-14-7", HEADING, clip),
                Line::Row(r) => {
                    let Value::Filter { mask, .. } = r.value else {
                        continue;
                    };
                    f.check(
                        format!("filter:{mask:x}"),
                        rect(14, y, 13, 13),
                        "",
                        filter & mask == mask,
                        true,
                    );
                    f.label(36, y + 2, r.caption, "15-6", INK, clip);
                }
            }
        }
        page.scroll_bar(&mut f, "chat-scroll", content(PageId::Chat, c), scroll, row);
        page.button(&mut f, 2, "chat-defaults", "Defaults", true);
        f
    }

    /// The Client page's Defaults: the settings host's rows as the classic page always set them,
    /// and every other row to the shared set's default.
    fn defaults(&mut self) -> Vec<PanelAction> {
        let s = self.draft.as_mut().unwrap();
        s.effects = s.sound_available;
        s.ambient = s.sound_available;
        s.interface = s.sound_available;
        s.stereo = true;
        s.effects_volume = 1.0;
        s.ambient_volume = 1.0;
        s.auto_degrade = true;
        s.performance = 0.5;
        s.brightness = 0.5;
        s.camera_stiffness = 0.23;
        s.full_screen = false;
        if let Some(i) = s.resolutions.iter().position(|r| *r == (1024, 768)) {
            s.resolution = i;
        }
        // The texture sizes and the detail textures go back to the shared set's defaults, as the
        // other interface's Defaults puts them.
        let default_of = |name: &str| {
            sheet::rows_for(PageId::Client, Face::Retail)
                .find(|r| r.preference() == Some(name))
                .and_then(|r| r.default)
                .map(PrefValue::from)
        };
        if let Some(PrefValue::Int(v)) = default_of("Render.LandscapeTextureDetail") {
            s.texture_levels[0] = texture_level(v);
        }
        if let Some(PrefValue::Int(v)) = default_of("Render.EnvironmentTextureDetail") {
            s.environment_very_high = v == 0;
            s.texture_levels[2] = texture_level(v);
        }
        if let Some(PrefValue::Bool(on)) = default_of("Render.BuildingDetailTextures") {
            s.environment_detail = on && s.detail_available;
        }
        if let Some(PrefValue::Bool(on)) = default_of("Render.LandscapeDetailTextures") {
            s.landscape_detail = on && s.detail_available;
        }
        let s = s.clone();
        for r in sheet::rows_for(PageId::Client, Face::Classic) {
            let names: Vec<&'static str> = match r.value {
                Value::Sound { on, volume } => vec![on, volume],
                _ => r.preference().into_iter().collect(),
            };
            // The interface stays the one the player is using: Defaults does not leave it.
            for p in names.into_iter().filter(|p| {
                !hosted(p) && *p != dereth_client_contract::options::interface::INTERFACE
            }) {
                let v = if r.preference() == Some(p) {
                    r.default.map(PrefValue::from)
                } else {
                    Some(PrefValue::Float(1.0))
                };
                if let Some(v) = v {
                    self.set_own(p, v);
                }
            }
        }
        self.dirty = true;
        vec![PanelAction::Host(HostAction::DefaultClassicSettings(s))]
    }

    /// Apply: the settings host's rows through the host, every other changed row written as its
    /// preference.
    fn apply(&mut self) -> Vec<PanelAction> {
        self.dirty = false;
        let s = self.draft.clone().unwrap();
        self.saved = Some(s.clone());
        let mut out = vec![PanelAction::Host(HostAction::ApplyClassicSettings(s))];
        for (p, v) in std::mem::take(&mut self.own) {
            let _ = store::set_value(p, v.clone());
            out.push(PanelAction::Game(UiRequest::SetPreference(p, v)));
        }
        out
    }

    fn client_event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        match e {
            ControlEvent::Scroll { id, value } if id == "scroll" => {
                self.scroll = value.clamp(0, max_scroll(PageId::Client, c));
                vec![]
            }
            ControlEvent::Check { id, checked } => {
                let Some(p) = id.strip_prefix("row:").and_then(client_row) else {
                    return vec![];
                };
                if p.preference().is_some_and(|n| !era_has(c, n)) {
                    return vec![];
                }
                let Some(name) = p.preference() else {
                    return vec![];
                };
                let s = self.draft.as_mut().unwrap();
                match name {
                    "Sound.SoundDisabled" => s.effects = checked,
                    "Sound.AmbientSoundDisabled" => s.ambient = checked,
                    "Sound.InterfaceSoundDisabled" => s.interface = checked,
                    "Render.AutomaticDegrades" => s.auto_degrade = checked,
                    "Render.BuildingDetailTextures" => s.environment_detail = checked,
                    "Display.FullScreen" => s.full_screen = checked,
                    other => self.set_own(other, PrefValue::Bool(checked)),
                }
                self.dirty = true;
                vec![]
            }
            ControlEvent::Value { id, value } => {
                let v = value.clamp(0, 100) as f32 / 100.0;
                let (kind, name) = id.split_once(':').unwrap_or_default();
                let Some(row) = client_row(name) else {
                    return vec![];
                };
                let name: &'static str = match (kind, row.value) {
                    ("vol", Value::Sound { volume, .. }) => volume,
                    (_, Value::Slider(p)) => p,
                    _ => return vec![],
                };
                let s = self.draft.as_mut().unwrap();
                self.dirty = true;
                match name {
                    "Sound.SoundVolume" => s.effects_volume = v,
                    "Sound.AmbientSoundVolume" => s.ambient_volume = v,
                    "Render.ScreenBrightness" => s.brightness = v,
                    "Camera.Stiffness" => s.camera_stiffness = v,
                    "Render.GraphicsPerformance" => s.performance = v,
                    other => {
                        let (lo, hi) = range(other);
                        self.set_own(other, PrefValue::Float(lo + v * (hi - lo)));
                        return vec![];
                    }
                }
                if matches!(
                    name,
                    "Render.ScreenBrightness" | "Camera.Stiffness" | "Render.GraphicsPerformance"
                ) {
                    vec![PanelAction::Host(HostAction::PreviewClassicSettings(
                        self.draft.clone().unwrap(),
                    ))]
                } else {
                    vec![]
                }
            }
            ControlEvent::Select { id, index } => {
                let Some(name) = id
                    .strip_prefix("row:")
                    .and_then(|p| client_row(p))
                    .and_then(Row::preference)
                else {
                    return vec![];
                };
                let s = self.draft.as_mut().unwrap();
                match name {
                    "Sound.SoundFeatures" => s.stereo = index == 0,
                    "Display.Resolution" if index < s.resolutions.len() => s.resolution = index,
                    "Render.LandscapeTextureDetail" => {
                        s.texture_levels[0] = u8::try_from(index.min(3)).unwrap_or(3);
                    }
                    "Render.EnvironmentTextureDetail" => {
                        s.environment_very_high = index == 0;
                        s.texture_levels[2] =
                            u8::try_from(index.saturating_sub(1).min(3)).unwrap_or(3);
                    }
                    other => {
                        let choices = store::choice_rows(other).unwrap_or_default();
                        let Some(choice) = choices.get(index) else {
                            return vec![];
                        };
                        self.set_own(other, PrefValue::Int(choice.value));
                    }
                }
                self.dirty = true;
                vec![]
            }
            ControlEvent::Activate(id) => match id.as_str() {
                "apply" => self.apply(),
                "reset" => {
                    self.dirty = false;
                    self.own.clear();
                    self.draft.clone_from(&self.saved);
                    vec![PanelAction::Host(HostAction::ResetClassicSettings)]
                }
                "defaults" => self.defaults(),
                _ => vec![],
            },
            _ => vec![],
        }
    }

    fn chat_event(&mut self, e: &ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        if let ControlEvent::Scroll { id, value } = e {
            if id == "chat-scroll" {
                self.chat_scroll = (*value).clamp(0, max_scroll(PageId::Chat, c));
            }
            return vec![];
        }
        let filter = Self::chat_filter(c);
        let mask = match e {
            ControlEvent::Check { id, checked } => {
                let Some(mask) = id
                    .strip_prefix("filter:")
                    .and_then(|m| u64::from_str_radix(m, 16).ok())
                else {
                    return vec![];
                };
                if *checked {
                    filter | mask
                } else {
                    filter & !mask
                }
            }
            ControlEvent::Activate(id) if id == "chat-defaults" => {
                sheet::MAIN_WINDOW_DEFAULT_FILTER
            }
            _ => return vec![],
        };
        vec![PanelAction::Game(UiRequest::SetChatWindowFilter {
            window: sheet::window::MAIN,
            mask,
        })]
    }
}

impl Panel for Settings {
    fn id(&self) -> &'static str {
        "options"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = translated(
            match self.tab {
                3 => self.chat(c),
                2 => self.client(c),
                1 => self.character.frame(c),
                _ => self.general(c),
            },
            25,
            crate::panels::side_height(),
        );
        // The four pages' tabs, as the classic window draws its tabs.
        for (k, (id, title, x, w)) in [
            ("general", "Options", 0, 68),
            ("character", "Character", 68, 78),
            ("chat", "Chat", 146, 54),
            ("sound", "Client", 200, 76),
        ]
        .into_iter()
        .enumerate()
        {
            let b = f.button(id, rect(x, 0, w, 25), title, true);
            let selected = match self.tab {
                1 => k == 1,
                2 => k == 3,
                3 => k == 2,
                _ => k == 0,
            };
            b.images = Some(
                [
                    if selected { "0600128F" } else { "06001291" },
                    "0600128F",
                    "06001291",
                ]
                .map(String::from),
            );
            b.endcaps = Some(
                [
                    if selected { "06001290" } else { "06001292" },
                    "06001290",
                    "06001292",
                ]
                .map(String::from),
            );
        }
        image_button(
            &mut f,
            "close",
            rect(276, 0, 24, 25),
            [0x06001283, 0x06001282, 0x06001283],
            true,
        );
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        let host_event = matches!(&e, ControlEvent::Activate(id)
            if matches!(id.as_str(), "general" | "character" | "chat" | "sound" | "close"));
        if self.tab == 1 && !host_event {
            let e = match e {
                ControlEvent::Pointer { x, y, pressed } => ControlEvent::Pointer {
                    x,
                    y: y - 25,
                    pressed,
                },
                e => e,
            };
            return self.character.event(e, c);
        }
        if self.tab == 3 && !host_event {
            return self.chat_event(&e, c);
        }
        // Nothing changed here yet: the draft starts from the settings as they are now, which the
        // other interface may have changed since the page was last used.
        if self.draft.is_none() || !self.dirty {
            self.draft = Some(c.settings.clone());
            self.saved = Some(c.settings.clone());
        }
        if self.tab == 2 && !host_event {
            return self.client_event(e, c);
        }
        let ControlEvent::Activate(id) = e else {
            return vec![];
        };
        match id.as_str() {
            "general" => {
                self.tab = 0;
                vec![]
            }
            "sound" => {
                self.tab = 2;
                vec![]
            }
            "chat" => {
                self.tab = 3;
                vec![]
            }
            "character" => {
                self.tab = 1;
                self.character.event(ControlEvent::Tick, c)
            }
            "close" => vec![PanelAction::Close],
            "leave" => vec![PanelAction::Confirm {
                id: "leave-world".into(),
                text: "\n\nThis will exit your character from the game world.\n\nAre you sure?"
                    .into(),
                accept: request(UiRequest::EndCharacterSession { ask: false }),
            }],
            "exit" => vec![PanelAction::Confirm {
                id: "exit-game".into(),
                text: "\n\nThis will exit your character from the game world and close the \
                       game.\n\nAre you sure?"
                    .into(),
                accept: vec![PanelAction::Host(HostAction::Quit)],
            }],
            "keyboard" => vec![PanelAction::Confirm {
                id: "configure-keyboard".into(),
                text: "\n\nTo configure your keyboard, you need to leave the world.\nProceed?"
                    .into(),
                accept: vec![
                    PanelAction::Game(UiRequest::EndCharacterSession { ask: false }),
                    PanelAction::Open("keyboard".into()),
                ],
            }],
            "urgent" => vec![PanelAction::Open("urgent-assistance".into())],
            "abuse" => vec![PanelAction::Open("abuse".into())],
            _ => vec![],
        }
    }
}
