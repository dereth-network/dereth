//! The classic interface's screens before the world: startup, character selection and the
//! character creation wizard.
mod appearance;
pub mod data;
mod model;
mod presentation;
mod startup;
#[cfg(test)]
mod tests;
pub mod wire;
use super::*;
use crate::int::{i32_from, u32_from};
use data::*;
use dereth_client_contract::pregame::{CharGenAction, CharacterAction};
use dereth_primitives::num::to_i32_f64;
pub use model::format_name;
use model::Creation;

pub const IDS: &[&str] = &[
    "login",
    "startup",
    "create-heritage",
    "create-sex",
    "create-appearance",
    "create-clothing",
    "create-heraldry",
    "create-profession",
    "create-attributes",
    "create-skills",
    "create-starting-spells",
    "create-name-summary",
    "credits",
    "enter-confirmation",
    "keyboard",
    "key-edit",
];
const ATTRS: [&str; 6] = [
    "Strength",
    "Endurance",
    "Coordination",
    "Quickness",
    "Focus",
    "Self",
];
const COLOR: u32 = 0xffd2d2c8;
const WHITE: u32 = 0xffff_ffff;
/// A character slot's height on the character screen.
const SLOT: i32 = 16;

pub fn make(id: &str) -> Option<Box<dyn Panel>> {
    let &external = IDS
        .iter()
        .find(|&&v| v == id.strip_prefix("pregame/").unwrap_or(id))?;
    let page = external.strip_prefix("create-").unwrap_or(external);
    Some(Box::new(Pregame::new(page, crate::art::creation())))
}

#[derive(Clone, Copy, Debug)]
enum KeyTransition {
    Done,
    Scheme(usize),
}
#[derive(Debug)]
struct Pregame {
    page: &'static str,
    startup: startup::Startup,
    startup_error: Option<String>,
    data: Result<std::sync::Arc<CreationData>, String>,
    state: Creation,
    selected: Option<usize>,
    selected_slot: Option<usize>,
    heritage_chosen: bool,
    sex_chosen: bool,
    delete_name: String,
    waiting: bool,
    response_notice: u32,
    set_notice: u32,
    created_name: Option<String>,
    creation_verified: bool,
    status: String,
    keyboard_row: Option<usize>,
    key_slot: usize,
    capture_before: u64,
    save_scheme: Option<String>,
    key_transition: Option<KeyTransition>,
    key_saved_transition: bool,
    skill_scroll: i32,
    profession_scroll: i32,
    skill_help_scroll: i32,
    /// How far the character screen's message box is scrolled, in pixels.
    message_scroll: i32,
    /// How far the character list is scrolled, in pixels, when the server allows more
    /// characters than the panel's five slots.
    character_scroll: i32,
    /// How far each clothing colour strip is scrolled, in pixels.
    color_scroll: [i32; 4],
    help_scroll: i32,
    keyboard_scroll: i32,
    credits_started: std::time::Instant,
    credits_variant: bool,
    creation_slot: Option<i32>,
}
/// The link-status indicator in the screen's top-right socket: green rings while the server has
/// been heard from in the last 5 seconds, grey up to 20 seconds, red after that or with no link.
fn link_indicator(f: &mut PanelFrame, c: &Context<'_>, x: i32, y: i32) {
    let did = match c.game.link_status() {
        Some(t) if t <= 5.0 => 0x0600_123c,
        Some(t) if t <= 20.0 => 0x0600_123b,
        _ => 0x0600_1229,
    };
    f.image_native(&format!("{did:08X}"), x, y, rect(x, y, 64, 64), true);
}
/// The delete field's prompt, which clicking into the field selects so typing replaces it.
const DELETE_PLACEHOLDER: &str = "<Enter Character Name To Delete>";
fn art_button(
    f: &mut PanelFrame,
    id: &str,
    r: crate::widgets::Rect,
    images: [u32; 3],
    enabled: bool,
) {
    let c = f.button(id, r, "", enabled);
    c.images = Some(images.map(|v| format!("{v:08X}")));
    c.keyed = false;
}
/// A creation-screen dropdown with its face's top left at `(x, y)`: the 117x25 face and the
/// 26-pixel arrow beside it, in dark type.
fn art_choice(
    f: &mut PanelFrame,
    id: impl Into<String>,
    x: i32,
    y: i32,
    options: Vec<String>,
    selected: usize,
) -> &mut Control {
    let c = f.control(
        id,
        rect(x, y, 117 + 26, 26),
        ControlKind::Choice { options, selected },
        true,
    );
    c.choice_art = true;
    c.font = "15-6".into();
    c.color = 0xff08_0808;
    c
}
/// A list of plated choices (heritages, sexes): each 249x64 row its icon at the left, drawn as
/// it is, a plate beside it (lit when chosen) with the name on it in large white type, and a white
/// rectangle round the chosen row.
fn plate_list<'a>(
    f: &mut PanelFrame,
    id: &str,
    bounds: crate::widgets::Rect,
    rows: impl ExactSizeIterator<Item = (&'a str, u32)>,
    selected: Option<usize>,
) {
    const ROW: i32 = 64;
    f.control(
        id,
        bounds,
        ControlKind::HitList {
            row_count: rows.len(),
            row_height: ROW,
            selected,
            offset: 0,
        },
        true,
    );
    let clip = [bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h];
    for (i, (name, icon)) in rows.enumerate() {
        let y = bounds.y + i32_from(i) * ROW;
        if y >= bounds.y + bounds.h {
            break;
        }
        let chosen = selected == Some(i);
        clipped(f, icon, rect(bounds.x, y, ROW, ROW), clip, false);
        let plate = rect(bounds.x + ROW, y, 185, ROW);
        f.image_native(
            if chosen { "06000F4D" } else { "06000F4C" },
            plate.x,
            plate.y,
            plate.intersect(bounds).unwrap_or_default(),
            false,
        );
        line(
            f,
            rect(plate.x, y + 12, 185, 30),
            name,
            "times-35-13-bold",
            WHITE,
            TextAlign::Center,
            clip,
        );
        if chosen {
            outline(f, rect(bounds.x, y, 249, ROW), WHITE);
        }
    }
}
/// `did` drawn to fill `r`, cut to `clip`.
fn clipped(f: &mut PanelFrame, did: u32, r: crate::widgets::Rect, clip: [i32; 4], keyed: bool) {
    f.image(&format!("{did:08X}"), r, false, keyed);
    if let Some(crate::Command::Image { clip: c, .. }) = f.screen.commands.last_mut() {
        *c = Some(clip);
    }
}
/// One line of `text` centred from top to bottom in `r`, cut to `clip`.
fn line(
    f: &mut PanelFrame,
    r: crate::widgets::Rect,
    text: impl Into<String>,
    font: &str,
    color: u32,
    align: TextAlign,
    clip: [i32; 4],
) {
    let height = crate::renderer::font_line_height(font).unwrap_or(r.h);
    f.text_box(
        rect(r.x, r.y + (r.h - height) / 2, r.w, height),
        text,
        font,
        color,
        align,
        false,
        Some(clip),
    );
}
/// A one-pixel rectangle round `r`.
fn outline(f: &mut PanelFrame, r: crate::widgets::Rect, color: u32) {
    for side in [
        rect(r.x, r.y, r.w, 1),
        rect(r.x, r.y + r.h - 1, r.w, 1),
        rect(r.x, r.y, 1, r.h),
        rect(r.x + r.w - 1, r.y, 1, r.h),
    ] {
        f.fill(side, color);
    }
}
/// How many of the vial's 108 rows show the full art: the share of the credits still to spend
/// above the minimum of ten in each of the six attributes.
fn vial_height(remaining: i32, total: i32) -> i32 {
    let spendable = total - 60;
    if spendable <= 0 {
        return 0;
    }
    to_i32_f64((108.0 * f64::from(remaining) / f64::from(spendable)).trunc()).clamp(0, 108)
}
fn title(f: &mut PanelFrame, text: &str) {
    f.text_box(
        rect(405, 0, 350, 39),
        text,
        "times-35-16-heavy",
        0xff080808,
        TextAlign::Center,
        false,
        None,
    );
}
fn text(f: &mut PanelFrame, r: crate::widgets::Rect, value: impl Into<String>) {
    f.text_box(r, value, "16-7", COLOR, TextAlign::Left, true, None);
}

impl Pregame {
    fn new(page: &'static str, data: Result<std::sync::Arc<CreationData>, String>) -> Self {
        let mut state = Creation::default();
        state.enter_preview_page(page);
        if let Ok(d) = &data {
            state.constrain(d);
        }
        Self {
            page,
            startup: startup::Startup::default(),
            startup_error: None,
            data,
            state,
            selected: None,
            selected_slot: None,
            heritage_chosen: !matches!(page, "login" | "heritage"),
            sex_chosen: !matches!(page, "login" | "heritage" | "sex"),
            delete_name: DELETE_PLACEHOLDER.into(),
            waiting: false,
            response_notice: 0,
            set_notice: 0,
            created_name: None,
            creation_verified: false,
            status: String::new(),
            keyboard_row: None,
            key_slot: 0,
            capture_before: 0,
            save_scheme: None,
            key_transition: None,
            key_saved_transition: false,
            skill_scroll: 0,
            profession_scroll: 0,
            skill_help_scroll: 0,
            message_scroll: 0,
            character_scroll: 0,
            color_scroll: [0; 4],
            help_scroll: 0,
            keyboard_scroll: 0,
            credits_started: std::time::Instant::now(),
            credits_variant: false,
            creation_slot: None,
        }
    }
    fn sequence(&self, d: &CreationData) -> Vec<&'static str> {
        let mut pages = vec!["heritage", "sex", "appearance", "clothing"];
        if self.state.heraldry.is_some() {
            pages.push("heraldry");
        }
        pages.extend(["profession", "attributes", "skills"]);
        if self.state.sex(d).legacy_60 > 0 && self.state.skills.get(&17).is_some_and(|&v| v >= 2) {
            pages.push("starting-spells");
        }
        pages.push("name-summary");
        pages
    }
    /// Menu: after the player agrees to lose the character's choices, back to the character list.
    fn leave_creation() -> PanelAction {
        PanelAction::Confirm {
            id: "cancel-creation".into(),
            text: "\n\nAre you sure you want to lose all the choices you made for this character?"
                .into(),
            accept: vec![PanelAction::Control(ControlEvent::Activate(
                "cancel-confirmed".into(),
            ))],
        }
    }
    fn navigate(&mut self, direction: isize) {
        if let Ok(d) = &self.data {
            let seq = self.sequence(d);
            if let Some(i) = seq.iter().position(|&p| p == self.page) {
                let n = i as isize + direction;
                if n >= 0 && (n as usize) < seq.len() {
                    self.page = seq[n as usize];
                    self.state.enter_preview_page(self.page);
                    self.help_scroll = 0;
                }
            }
        }
    }
    fn background(&self, f: &mut PanelFrame) {
        for (did, r, tile) in [
            (0x060004bf, rect(0, 88, 47, 379), true),
            (0x060004bc, rect(318, 534, 482, 66), false),
            (
                if self.page == "attributes" {
                    0x060004bb
                } else {
                    0x060004c0
                },
                rect(318, 60, 482, 474),
                false,
            ),
            (0x060004bd, rect(318, 0, 482, 60), false),
            (0x060004c3, rect(0, 0, 318, 88), true),
            (0x060004c1, rect(0, 467, 318, 133), false),
        ] {
            f.image(&format!("{did:08X}"), r, tile, false);
        }
    }
    /// The preview's rotate buttons and face-zoom check, on every page whether or not there is a
    /// model to show yet.
    fn preview_controls(&self, f: &mut PanelFrame) {
        art_button(
            f,
            "rotate-left",
            rect(27, 483, 43, 43),
            [0x600028b, 0x6000287, 0x60004fd],
            true,
        );
        art_button(
            f,
            "rotate-right",
            rect(275, 482, 43, 43),
            [0x6000286, 0x6000285, 0x60004ff],
            true,
        );
        let c = f.check(
            "zoom-face",
            rect(268, 529, 40, 43),
            "",
            self.state.zoom_face,
            true,
        );
        c.images = Some(["06000288".into(), "0600028A".into(), "06000288".into()]);
    }
    /// The model preview, once both heritage and sex are chosen.
    fn preview(&self, f: &mut PanelFrame, d: &CreationData) {
        if !self.heritage_chosen || !self.sex_chosen {
            return;
        }
        let s = self.state.sex(d);
        let state = &self.state;
        let hair = s.hair_styles.get(state.hair_style);
        let bald = hair.is_some_and(|h| h.bald != 0);
        let mut overlays = vec![];
        if let Some(h) = hair {
            overlays.push(h.appearance.clone());
        }
        if let Some(e) = s.eyes.get(state.face[0]) {
            overlays.push(if bald && !e.bald_appearance.is_empty() {
                e.bald_appearance.clone()
            } else {
                e.appearance.clone()
            });
        }
        if let Some(n) = s.noses.get(state.face[1]) {
            overlays.push(n.appearance.clone());
        }
        if let Some(m) = s.mouths.get(state.face[2]) {
            overlays.push(m.appearance.clone());
        }
        let appearance = Appearance {
            heraldry: s.heraldry(state.heraldry, state.heraldry_color),
            base_palette_id: s.base_palette,
            setup_id: s.setup,
            environment_setup_id: d.heritages[state.heritage].environment,
            base_objdesc_hex: s.appearance.clone(),
            appearance_overlays_hex: overlays,
            skin_palette_set: s.skin_palette,
            skin_shade: state.skin_shade,
            hair_palette_set: s.hair_colors.get(state.hair_color).copied().unwrap_or(0),
            hair_shade: state.hair_shade,
            eye_palette_id: s.eye_colors.get(state.eye_color).copied().unwrap_or(0),
            clothing: [0, 2, 1, 3]
                .into_iter()
                .filter_map(|i| {
                    s.clothes(i).get(state.styles[i]).map(|c| Clothing {
                        table_id: c.icon,
                        palette_template: state.clothing_color(d, i),
                        shade: state.shades[i],
                    })
                })
                .collect(),
            heading_degrees: 150.0,
            rotation_velocity: state.rotation_velocity,
            zoom_face: state.zoom_face,
            show_clothes: true,
        };
        f.preview(Preview {
            kind: PreviewKind::CharGen,
            rect: rect(45, 43, 273, 424),
            object: None,
            appearance: Some(appearance),
        });
    }
    fn navigation(&self, f: &mut PanelFrame, d: &CreationData) {
        // The wizard's buttons along the bottom, left to right: Help, Menu, Back, Next and To End
        // (Done instead of Next and To End on the last page). Their captions are in the art.
        let seq = self.sequence(d);
        let i = seq.iter().position(|&p| p == self.page).unwrap_or(0);
        let last = self.page == "name-summary";
        art_button(
            f,
            "creation-help",
            rect(335, 537, 100, 57),
            [0x6000292, 0x600029f, 0x6000297],
            !self.waiting,
        );
        art_button(
            f,
            "creation-menu",
            rect(445, 536, 84, 57),
            [0x600028f, 0x60002a0, 0x6000298],
            !self.waiting,
        );
        art_button(
            f,
            "back",
            rect(529, 536, 80, 57),
            [0x6000299, 0x600029c, 0x6000296],
            !self.waiting,
        );
        if last {
            art_button(
                f,
                "create-submit",
                rect(648, 534, 119, 57),
                [0x6000291, 0x600029e, 0x6000291],
                !self.waiting
                    && !self.state.name.trim().is_empty()
                    && self.state.name != "Enter name",
            );
        } else {
            art_button(
                f,
                "next",
                rect(609, 536, 82, 57),
                [0x6000294, 0x60002a1, 0x600029a],
                i + 1 < seq.len()
                    && !self.waiting
                    && match self.page {
                        "heritage" => self.heritage_chosen,
                        "sex" => self.sex_chosen,
                        "profession" => self.state.template < self.state.sex(d).templates.len(),
                        _ => true,
                    },
            );
            art_button(
                f,
                "summary",
                rect(691, 536, 93, 57),
                [0x6000295, 0x6000293, 0x600029b],
                !self.waiting && self.heritage_chosen && self.sex_chosen,
            );
            art_button(
                f,
                "random",
                rect(71, 498, 197, 90),
                [0x6000283, 0x6000282, 0x6000283],
                !self.waiting,
            );
            let caption = match self.page {
                "heritage" => "Random\nHeritage Group",
                "sex" => "Random\nSex",
                "appearance" => "Random\nAppearance",
                "clothing" => "Random\nClothing",
                "heraldry" => "Random\nSymbol",
                "profession" | "custom" => "Random\nProfession",
                "attributes" => "Random\nAttributes",
                "skills" => "Random\nSkills",
                "starting-spells" => "Random\nSpells",
                _ => "Random",
            };
            f.text_box(
                rect(92, 518, 150, 48),
                caption,
                "16-7",
                0xff08_0808,
                TextAlign::Center,
                true,
                None,
            );
        }
    }
    fn help(&self, f: &mut PanelFrame, d: &CreationData, did: u32) {
        if let Some(t) = d.help_text.get(&did.to_string()) {
            let height = if matches!(self.page, "appearance" | "attributes") {
                70
            } else {
                98
            };
            let full = d
                .text_heights
                .get(&did.to_string())
                .copied()
                .unwrap_or(2000);
            let offset = self.help_scroll.min((full - height).max(0));
            f.text_box(
                rect(383, 64 - offset, 401, full.max(height)),
                t,
                "16-7",
                COLOR,
                TextAlign::Left,
                true,
                Some([383, 64, 784, 64 + height]),
            );
            f.control(
                "help",
                rect(383, 64, 401, height),
                ControlKind::HitList {
                    row_count: full as usize,
                    row_height: 1,
                    selected: None,
                    offset,
                },
                true,
            );
            f.control(
                "help-scroll",
                rect(784, 64, 16, height),
                ControlKind::ScrollBar {
                    min: 0,
                    max: (full - height).max(0),
                    value: offset,
                    page: height,
                    step: 16,
                    vertical: true,
                    arrow_size: 16,
                    thumb_size: 16,
                },
                true,
            );
        }
    }
    fn login(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = PanelFrame::new(800, 600);
        f.fill(rect(0, 0, 800, 600), 0xff000000);
        for (did, r) in [
            (
                crate::art::installed()
                    .map_or(crate::art::CHARACTER_PANEL, |a| a.character_panel()),
                rect(0, 0, 282, 600),
            ),
            (0x600123a, rect(282, 0, 518, 67)),
            (0x6001231, rect(746, 563, 28, 21)),
            (0x6001232, rect(746, 264, 28, 21)),
            (0x6001234, rect(281, 563, 28, 21)),
            (0x6001233, rect(281, 264, 28, 21)),
            (0x6001235, rect(282, 285, 11, 278)),
            (0x6001235, rect(763, 285, 11, 278)),
            (0x6001237, rect(309, 262, 437, 23)),
            (0x6001236, rect(309, 563, 437, 16)),
        ] {
            f.image(&format!("{did:08X}"), r, false, false);
        }
        f.text_box(
            rect(200, 10, 400, 39),
            "Asheron's Call",
            "times-35-16-heavy",
            0xff080808,
            TextAlign::Center,
            false,
            None,
        );
        // The trademark sign: small type centred in its own box, clear of the title's end.
        f.text_box(
            rect(490, 10, 60, 17),
            "TM",
            "10-4",
            0xff080808,
            TextAlign::Center,
            false,
            None,
        );
        link_indicator(&mut f, c, 725, 4);
        // The plates behind the Credits, Keyboard and Exit buttons are wider than the buttons.
        for (did, y) in [(0x060019e3, 95), (0x0600122c, 148), (0x0600122d, 197)] {
            f.image_native(&format!("{did:08X}"), 649, y, rect(649, y, 157, 55), false);
        }
        let chars = presentation::characters(c.pregame);
        // The panel's art frames six character slots, 16 pixels apart from y 136, and each slot
        // is drawn with the panel behind it. A server that allows more characters than six gets
        // the same six slots and, beside them, a scroll bar drawn as the message box's is.
        // The list and its bar keep left of the Enter button, whose picture would cover the bar.
        let list = rect(50, 136, 144, 6 * SLOT);
        let max = (crate::int::i32_from(chars.len()) * SLOT - list.h).max(0);
        let offset = self.character_scroll.clamp(0, max);
        f.control(
            "characters",
            list,
            ControlKind::HitList {
                row_count: chars.len(),
                row_height: SLOT,
                selected: self.selected,
                offset,
            },
            true,
        );
        let clip = [list.x, list.y, list.x + list.w, list.y + list.h];
        for (i, v) in chars.iter().enumerate() {
            let y = list.y + crate::int::i32_from(i) * SLOT - offset;
            if y + SLOT <= list.y || y >= list.y + list.h {
                continue;
            }
            if self.selected == Some(i) {
                if let Some(b) = rect(list.x, y, list.w, SLOT).intersect(list) {
                    f.fill(b, 0xff27_4657);
                }
            }
            let color = if v.character.is_some_and(|ch| ch.seconds_grace_period > 0) {
                0xffff_0046
            } else {
                COLOR
            };
            f.label(
                list.x + 2,
                y,
                v.character.map_or("", |ch| ch.name.as_str()),
                "14-6",
                color,
                Some(clip),
            );
        }
        if max > 0 {
            f.control(
                "characters-scroll",
                rect(list.x + list.w + 1, list.y, 20, list.h),
                ControlKind::ScrollBar {
                    min: 0,
                    max,
                    value: offset,
                    page: list.h,
                    step: SLOT,
                    vertical: true,
                    arrow_size: 20,
                    thumb_size: 20,
                },
                true,
            );
        }
        let selected = self
            .selected
            .and_then(|i| chars.get(i))
            .and_then(|v| v.character);
        let live = selected.is_some_and(|v| v.seconds_grace_period == 0);
        art_button(
            &mut f,
            "enter",
            rect(216, 78, 185, 183),
            [0x600122f, 0x6001230, 0x600122e],
            live && !self.waiting,
        );
        let room = presentation::first_empty_slot(c.pregame).is_some();
        f.button(
            "create",
            rect(402, 119, 220, 36),
            "Create Character",
            room && !self.waiting,
        );
        f.button(
            "quick",
            rect(402, 157, 220, 36),
            "Quick Character",
            room && !self.waiting,
        );
        f.button(
            if live { "delete" } else { "restore" },
            rect(402, 195, 220, 36),
            if live {
                "Delete Character"
            } else {
                "Restore Character"
            },
            selected.is_some() && !self.waiting,
        );
        if live {
            f.image("060004E4", rect(410, 230, 205, 25), false, false);
            f.edit(
                "delete-name",
                rect(410, 230, 205, 25),
                &self.delete_name,
                32,
                false,
                !self.waiting,
            )
            .select_on_focus = true;
        }
        art_button(
            &mut f,
            "credits",
            rect(649, 95, 105, 55),
            [0x60019e3, 0x60019e2, 0x60019e3],
            true,
        );
        art_button(
            &mut f,
            "keyboard",
            rect(649, 148, 105, 55),
            [0x600122c, 0x600122a, 0x600122c],
            true,
        );
        art_button(
            &mut f,
            "quit",
            rect(649, 197, 105, 55),
            [0x600122d, 0x600122b, 0x600122d],
            true,
        );
        // The message box: parchment, with the world's character screen message (or this client's
        // own welcome text, when one is configured) until a status or error replaces it.
        f.image("06001115", rect(293, 285, 450, 278), true, false);
        let mut status = self.status.clone();
        if let Some(e) = &c.pregame.error {
            status = e.clone();
        }
        if status.is_empty() {
            // The world's own message first, as this interface's era showed it.
            status = if let Some(text) = &c.pregame.character_screen_message {
                text.clone()
            } else if c.classic.welcome.is_empty() {
                "Receiving system messages...".into()
            } else {
                c.classic.welcome.clone()
            };
        }
        // Light type, as the era's, scrolled by the bar beside the box.
        let height = 278 - 4;
        let full = crate::renderer::measure_text_height("16-7", &status, 446).unwrap_or(0);
        let offset = self.message_scroll.min((full - height).max(0));
        f.text_box(
            rect(295, 287 - offset, 446, full.max(height)),
            status,
            "16-7",
            COLOR,
            TextAlign::Left,
            true,
            Some([293, 287, 743, 287 + height]),
        );
        f.control(
            "message",
            rect(293, 285, 450, 278),
            ControlKind::HitList {
                row_count: full.max(0) as usize,
                row_height: 1,
                selected: None,
                offset,
            },
            true,
        );
        f.control(
            "message-scroll",
            rect(743, 285, 20, 278),
            ControlKind::ScrollBar {
                min: 0,
                max: (full - height).max(0),
                value: offset,
                page: height,
                step: 16,
                vertical: true,
                arrow_size: 20,
                thumb_size: 20,
            },
            true,
        );
        f
    }
    fn creation_frame(&self, d: &CreationData, c: &Context<'_>) -> PanelFrame {
        let mut f = PanelFrame::new(800, 600);
        self.background(&mut f);
        link_indicator(&mut f, c, 736, 0);
        self.preview(&mut f, d);
        self.preview_controls(&mut f);
        self.navigation(&mut f, d);
        let state = &self.state;
        let sex = state.sex(d);
        let heritage = &d.heritages[state.heritage];
        if let Some(index) = match self.page {
            "sex" => Some(0),
            "appearance" => Some(1),
            "clothing" => Some(2),
            "heraldry" => Some(3),
            "attributes" => Some(4),
            "skills" => Some(5),
            "starting-spells" => Some(6),
            "name-summary" => Some(7),
            _ => None,
        } {
            if let Some(&did) = d.help_ids.get(index) {
                self.help(&mut f, d, did);
            }
        }
        match self.page {
            "heritage" => {
                title(&mut f, "Heritage Group");
                self.help(&mut f, d, heritage.description);
                text(&mut f, rect(451, 256, 300, 20), "Available Heritage Groups");
                plate_list(
                    &mut f,
                    "heritage",
                    rect(451, 276, 300, 192),
                    d.heritages.iter().map(|v| (v.name.as_str(), v.icon)),
                    self.heritage_chosen.then_some(state.heritage),
                );
            }
            "sex" => {
                title(&mut f, "Sex");

                text(&mut f, rect(444, 266, 300, 20), "Available Sexes");
                plate_list(
                    &mut f,
                    "sex",
                    rect(444, 286, 300, 192),
                    heritage.sexes.iter().map(|v| (v.name.as_str(), v.icon)),
                    self.sex_chosen.then_some(state.sex),
                );
            }
            "profession" => {
                title(&mut f, "Profession");
                if let Some(t) = sex.templates.get(state.template) {
                    self.help(&mut f, d, t.resource);
                }
                self.profession_frame(&mut f, d);
            }
            "attributes" => {
                title(&mut f, "Attributes");
                // The credit vial: the full (red) art over the empty, shown from the bottom up to
                // the share of credits left, with the count of them on top.
                f.image_native("060002CB", 379, 141, rect(379, 141, 128, 108), false);
                let full = vial_height(state.remaining_attributes(d), sex.attribute_credits);
                f.image_native(
                    "060002CA",
                    379,
                    141,
                    rect(379, 141 + 108 - full, 128, full),
                    false,
                );
                f.text_box(
                    rect(454, 225, 44, 26),
                    state.remaining_attributes(d).to_string(),
                    "16-7",
                    COLOR,
                    TextAlign::Left,
                    false,
                    None,
                );
                for (i, y) in [282, 321, 361, 402, 441, 481].into_iter().enumerate() {
                    let skins = [
                        [0x2bc, 0x2c2, 0x2c8],
                        [0x2b8, 0x2be, 0x2c4],
                        [0x2bd, 0x2c3, 0x2c9],
                        [0x2ba, 0x2c0, 0x2c6],
                        [0x2b9, 0x2bf, 0x2c5],
                        [0x2bb, 0x2c1, 0x2c7],
                    ][i];
                    f.image(
                        &format!("{:08X}", 0x06000000 + skins[0]),
                        rect(382, y, 360, 25),
                        false,
                        false,
                    );
                    f.screen.commands.push(crate::Command::Image {
                        did: format!("{:08X}", 0x06000000 + skins[1]),
                        x: 382,
                        y,
                        width: 360,
                        height: 25,
                        // The fill stops at the thumb's centre.
                        clip: Some([382, y, 382 + 335 * state.attrs[i] / 100 + 12, y + 25]),
                        color_key: None,
                        key_bits: None,
                        tile: false,
                    });
                    f.image(
                        &format!("{:08X}", 0x06000000 + skins[2]),
                        rect(382 + 335 * state.attrs[i] / 100, y, 25, 25),
                        false,
                        false,
                    );
                    f.slider(
                        format!("attr-{i}"),
                        rect(382, y, 360, 25),
                        0,
                        100,
                        state.attrs[i],
                        1,
                    )
                    .paint = false;
                    f.image("060002B7", rect(757, y, 36, 25), false, false);
                    f.edit(
                        format!("attr-value-{i}"),
                        rect(757, y, 36, 25),
                        state.attrs[i].to_string(),
                        3,
                        false,
                        true,
                    );
                }
                f.label(
                    735,
                    160,
                    (state.attrs[1] / 2).to_string(),
                    "16-7",
                    COLOR,
                    None,
                );
                f.label(735, 191, state.attrs[1].to_string(), "16-7", COLOR, None);
                f.label(735, 222, state.attrs[5].to_string(), "16-7", COLOR, None);
            }
            "skills" => {
                title(&mut f, "Skills");
                self.skills_frame(&mut f, d);
            }
            "clothing" => {
                title(&mut f, "Clothing");
                self.clothing_frame(&mut f, d);
            }
            "appearance" => {
                title(&mut f, "Appearance");
                self.appearance_frame(&mut f, d);
            }
            "heraldry" => {
                title(&mut f, "Heraldry Symbol");
                f.list(
                    "heraldry",
                    rect(368, 177, 240, 320),
                    sex.legacy_80
                        .iter()
                        .enumerate()
                        .map(|(i, v)| ListRow {
                            text: (i + 1).to_string(),
                            icon: v.get(8).copied().map(DataId),
                            color: COLOR,
                        })
                        .collect(),
                    state.heraldry,
                    40,
                );
                for y in [222, 285] {
                    f.label(
                        646,
                        y,
                        if y == 222 {
                            "Foreground Color"
                        } else {
                            "Background Color"
                        },
                        "16-7",
                        COLOR,
                        None,
                    );
                }
                f.slider(
                    "heraldry-color",
                    rect(634, 242, 152, 32),
                    0,
                    15,
                    i32_from(state.heraldry_color),
                    1,
                );
            }
            "starting-spells" => {
                title(&mut f, "Starting Spells");
                text(&mut f, rect(459, 199, 100, 20), "Spell Credits");
                f.label(561, 201, sex.legacy_60.to_string(), "16-7", COLOR, None);
                for (known, x) in [(false, 386), (true, 620)] {
                    text(
                        &mut f,
                        rect(x, 240, 150, 20),
                        if known {
                            "Known Spells"
                        } else {
                            "Available Spells"
                        },
                    );
                    f.list(
                        if known {
                            "known-spells"
                        } else {
                            "available-spells"
                        },
                        rect(x, 260, 150, 218),
                        sex.legacy_68
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| {
                                state.spells.get(*i).copied().unwrap_or(false) == known
                            })
                            .map(|(_, v)| v.name.clone().into())
                            .collect(),
                        None,
                        24,
                    );
                }
            }
            "name-summary" => {
                title(&mut f, "Name and Summary");

                text(&mut f, rect(385, 162, 150, 18), "Character Name");
                f.edit(
                    "name",
                    rect(385, 182, 132, 25),
                    &state.name,
                    32,
                    false,
                    !self.waiting,
                )
                .select_on_focus = true;
                text(&mut f, rect(616, 162, 150, 18), "Starting Town");
                let areas: Vec<_> = heritage
                    .primary_areas
                    .iter()
                    .chain(&heritage.secondary_areas)
                    .copied()
                    .collect();
                art_choice(
                    &mut f,
                    "area",
                    616,
                    182,
                    areas
                        .iter()
                        .filter_map(|&i| d.areas.get(i))
                        .map(|v| v.name.clone())
                        .collect(),
                    areas.iter().position(|&i| i == state.area).unwrap_or(0),
                );
                text(
                    &mut f,
                    rect(460, 250, 300, 160),
                    format!(
                        "{} {}\n{}\n{}",
                        heritage.name,
                        sex.name,
                        sex.templates
                            .get(state.template)
                            .map(|v| v.name.as_str())
                            .unwrap_or(""),
                        ATTRS
                            .iter()
                            .zip(state.attrs)
                            .map(|(n, v)| format!("{n}: {v}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ),
                );
                text(&mut f, rect(385, 410, 395, 100), &self.status);
            }
            _ => {}
        }
        if self.waiting {
            text(&mut f, rect(383, 480, 400, 35), "Please wait...");
            for c in &mut f.controls {
                c.enabled = false;
            }
        }
        f
    }
}

impl Panel for Pregame {
    fn id(&self) -> &'static str {
        IDS.iter()
            .copied()
            .find(|p| p.strip_prefix("create-").unwrap_or(p) == self.page)
            .unwrap_or("login")
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        if self.page == "login" {
            return self.login(c);
        }
        if self.page == "credits" {
            return self.credits(c);
        }
        if matches!(self.page, "keyboard" | "key-edit") {
            return self.keyboard(c);
        }
        if self.page == "startup" {
            return self.startup.paint(c.pregame);
        }
        if self.page == "enter-confirmation" {
            let mut f = PanelFrame::new(800, 600);
            f.image("06000508", rect(0, 0, 800, 600), true, false);
            f.button(
                "enter",
                rect(325, 20, 200, 36),
                "Enter Game",
                self.selected.is_some(),
            );
            f.button("login", rect(325, 70, 200, 36), "Back to Login", true);
            return f;
        }
        match &self.data {
            Ok(d) => self.creation_frame(d, c),
            Err(e) => {
                let mut f = PanelFrame::new(800, 600);
                text(
                    &mut f,
                    rect(30, 30, 740, 500),
                    format!("Creation data unavailable: {e}"),
                );
                f.button("login", rect(325, 540, 200, 36), "Back to Login", true);
                f
            }
        }
    }
    fn event(&mut self, event: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        let mut actions = vec![];
        if self.page == "credits"
            && matches!(
                event,
                ControlEvent::KeyPressed
                    | ControlEvent::Activate(_)
                    | ControlEvent::Pointer { pressed: true, .. }
            )
        {
            self.page = "login";
            return actions;
        }
        if let ControlEvent::DoubleClick { id, index } = &event {
            let mut result = self.event(
                ControlEvent::Select {
                    id: id.clone(),
                    index: *index,
                },
                c,
            );
            let follow = match id.as_str() {
                "characters" => {
                    presentation::characters(c.pregame)
                        .get(*index)
                        .map(|r| match r.character {
                            Some(ch) if ch.seconds_grace_period > 0 => "restore",
                            Some(_) => "enter",
                            None => "create",
                        })
                }
                "heritage" | "sex" | "profession" | "custom" => Some("next"),
                _ => None,
            };
            if let Some(id) = follow {
                result.extend(self.event(ControlEvent::Activate(id.into()), c));
            }
            return result;
        }
        if matches!(event, ControlEvent::Tick) {
            if self.key_saved_transition && !c.keyboard.dirty {
                self.key_saved_transition = false;
                match self.key_transition.take() {
                    Some(KeyTransition::Done) => {
                        if c.pregame.in_world {
                            actions.push(PanelAction::Close);
                        } else {
                            self.page = "login";
                        }
                    }
                    Some(KeyTransition::Scheme(i)) => {
                        actions.push(PanelAction::Host(HostAction::KeyboardScheme(u32_from(i))))
                    }
                    None => {}
                }
            }
            if self.page == "key-edit" && c.keyboard.capture_revision != self.capture_before {
                self.page = "keyboard";
            }
            if self.page == "credits" && self.credits_finished() {
                self.credits_variant = !self.credits_variant;
                self.page = "login";
            }
            if c.pregame.error.is_some() {
                self.waiting = false;
            }
            if matches!(self.page, "login" | "enter-confirmation")
                && (self.selected.is_none() || c.pregame.character_set_notices != self.set_notice)
            {
                let rows = presentation::characters(c.pregame);
                self.selected = self
                    .selected_slot
                    .and_then(|slot| rows.iter().position(|r| r.wire_slot == Some(slot)))
                    .or_else(|| {
                        rows.iter().position(|r| {
                            r.character.is_some_and(|ch| ch.seconds_grace_period == 0)
                        })
                    });
                self.selected_slot = self.selected.and_then(|i| rows[i].wire_slot);
            }
            if self.page == "startup" {
                if self.startup_error.is_none() && c.pregame.error.is_some() {
                    self.startup_error = c.pregame.error.clone();
                    if let Some(error) = &self.startup_error {
                        actions.push(PanelAction::Message {
                            id: "startup-error".into(),
                            text: error.clone(),
                            accept: vec![PanelAction::Host(HostAction::Quit)],
                        });
                    }
                }
                if self.startup_error.is_none() && self.startup.tick(c.pregame) {
                    self.page = "login";
                }
            }
            if c.pregame.character_set_notices != self.set_notice {
                self.set_notice = c.pregame.character_set_notices;
                self.waiting = false;
            }
            if c.pregame.chargen_response_notices != self.response_notice {
                self.response_notice = c.pregame.chargen_response_notices;
                self.creation_verified =
                    self.created_name.is_some() && c.pregame.chargen_response == Some(1);
                self.waiting = self.creation_verified;
                self.status = match c.pregame.chargen_response {
                    Some(1) if self.created_name.is_some() => "Waiting for character list...",
                    Some(1) => "",
                    Some(3) if self.page == "login" => "Another character already exists by that name. Your character cannot be restored.",
                    Some(5 | 6) if self.page == "login" => "That character cannot be restored.",
                    Some(3) => "That name is already in use.",
                    Some(4) => "That name is not permitted.",
                    Some(7) => "That name requires administrator privileges.",
                    _ => "The character database is unavailable.",
                }
                .into();
                if c.pregame.chargen_response != Some(1) {
                    self.created_name = None;
                }
            }
            if self.creation_verified {
                self.waiting = true;
                if let (Some(name), Some(set)) = (&self.created_name, &c.pregame.character_set) {
                    if let Some(ch) = set
                        .set
                        .iter()
                        // A server may prefix a privileged account's names with `+`.
                        .find(|v| {
                            v.id.0 != 0 && v.name.trim_start_matches('+').eq_ignore_ascii_case(name)
                        })
                    {
                        actions.push(PanelAction::Game(UiRequest::CharGenAction(
                            CharGenAction::LogOn(ch.id),
                        )));
                        self.created_name = None;
                        self.creation_verified = false;
                        self.status = "Entering the world...".into();
                    }
                }
            }
            return actions;
        }
        if self.waiting {
            return actions;
        }
        match event {
            ControlEvent::Select { id, index } => match id.as_str() {
                "characters" => {
                    let rows = presentation::characters(c.pregame);
                    self.selected = (index < rows.len()).then_some(index);
                    self.selected_slot = rows.get(index).and_then(|r| r.wire_slot);
                    self.delete_name = DELETE_PLACEHOLDER.into();
                }
                "bindings" => {
                    self.keyboard_row = c
                        .keyboard
                        .bindings
                        .get(index)
                        .filter(|b| b.action > 0 && b.action != u32::MAX)
                        .map(|_| index);
                }
                "scheme-saved-name" => {
                    if self.save_scheme.is_some() {
                        self.save_scheme = c.keyboard.schemes.get(index).cloned();
                    }
                }
                "scheme" => {
                    if c.keyboard.dirty && index != c.keyboard.scheme as usize {
                        self.key_transition = Some(KeyTransition::Scheme(index));
                        actions.push(PanelAction::Question {
                            id: "save-before-key-switch".into(),
                            text: "\n\nSave current changes?".into(),
                            accept: vec![PanelAction::Control(ControlEvent::Activate(
                                "key-save-transition".into(),
                            ))],
                            reject: vec![
                                PanelAction::Host(HostAction::KeyboardScheme(u32_from(index))),
                                PanelAction::Control(ControlEvent::Activate(
                                    "key-transition-cancel".into(),
                                )),
                            ],
                        });
                    } else {
                        actions.push(PanelAction::Host(HostAction::KeyboardScheme(u32_from(
                            index,
                        ))));
                    }
                }
                _ => {
                    if let Ok(d) = &self.data {
                        match id.as_str() {
                            "heritage" if index < d.heritages.len() => {
                                self.state.heritage = index;
                                self.heritage_chosen = true;
                                self.state.constrain(d);
                            }
                            "sex" if index < d.heritages[self.state.heritage].sexes.len() => {
                                self.state.sex = index;
                                self.sex_chosen = true;
                                self.state.constrain(d);
                            }
                            "profession" => self.state.apply_template(d, index + 1),
                            "custom" => self.state.apply_template(d, 0),
                            "skills" => {
                                self.skill_help_scroll = 0;
                                self.state.selected_skill =
                                    presentation::skill_rows(d, &self.state)
                                        .get(index)
                                        .and_then(|v| v.skill);
                            }
                            "area" => {
                                if let Some(&v) = d.heritages[self.state.heritage]
                                    .primary_areas
                                    .iter()
                                    .chain(&d.heritages[self.state.heritage].secondary_areas)
                                    .nth(index)
                                {
                                    self.state.area = v;
                                }
                            }
                            "heraldry" => {
                                if index < self.state.sex(d).legacy_80.len() {
                                    self.state.heraldry = Some(index);
                                }
                            }
                            "available-spells" | "known-spells" => {
                                let known = id == "known-spells";
                                if let Some(i) = self
                                    .state
                                    .spells
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, v)| **v == known)
                                    .nth(index)
                                    .map(|(i, _)| i)
                                {
                                    self.state.spell(d, i, !known);
                                }
                            }
                            _ => {
                                if let Some(i) = suffix(&id, "style-") {
                                    if i < 4 {
                                        let count = self.state.sex(d).clothes(i).len();
                                        if index < count + usize::from(i == 0) {
                                            self.state.styles[i] =
                                                if i == 0 { index.wrapping_sub(1) } else { index };
                                            self.state.colors[i] = self.state.colors[i].min(
                                                self.state
                                                    .clothing_colors(d, i)
                                                    .len()
                                                    .saturating_sub(1),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            ControlEvent::Scroll { id, value } => match id.as_str() {
                "skills" | "skills-scroll" => self.skill_scroll = value.max(0),
                "profession" | "profession-scroll" => self.profession_scroll = value.max(0),
                "skill-help" | "skill-help-scroll" => self.skill_help_scroll = value.max(0),
                "help" | "help-scroll" => self.help_scroll = value.max(0),
                "message" | "message-scroll" => self.message_scroll = value.max(0),
                "characters" | "characters-scroll" => self.character_scroll = value.max(0),
                "bindings" | "bindings-scroll" => self.keyboard_scroll = value.max(0),
                _ => {
                    // A colour strip scrolls a whole swatch at a time.
                    if let Some(i) = suffix(&id, "color-scroll-").filter(|&i| i < 4) {
                        self.color_scroll[i] = (value.max(0) + 16) / 32 * 32;
                    }
                }
            },
            ControlEvent::Edit { id, text } => match id.as_str() {
                "delete-name" => self.delete_name = text,
                "scheme-name" => self.save_scheme = Some(text),
                "name" => {
                    if text.len() <= 32 {
                        self.state.name = text;
                    }
                }
                _ => {
                    if let (Some(i), Ok(v), Ok(d)) = (
                        suffix(&id, "attr-value-").or_else(|| suffix(&id, "attr-")),
                        text.parse::<i32>(),
                        &self.data,
                    ) {
                        self.state.attribute(d, i, v);
                    }
                }
            },
            ControlEvent::Check { id, checked } => {
                if id == "zoom-face" {
                    self.state.zoom_face = checked;
                }
            }
            ControlEvent::Value { id, value } => {
                if let Ok(d) = &self.data {
                    if let Some(i) = suffix(&id, "attr-") {
                        self.state.attribute(d, i, value);
                    } else if let Some(i) = suffix(&id, "shade-") {
                        if i < 4 {
                            self.state.shades[i] = f64::from(value.clamp(0, 1000)) / 1000.;
                        }
                    } else if let Some(i) = suffix(&id, "face-") {
                        if i < 3 {
                            let n = [
                                self.state.sex(d).eyes.len(),
                                self.state.sex(d).noses.len(),
                                self.state.sex(d).mouths.len(),
                            ][i];
                            self.state.face[i] = (value.max(0) as usize).min(n.saturating_sub(1));
                        }
                    } else {
                        match id.as_str() {
                            "skin-shade" => {
                                self.state.skin_shade = f64::from(value.clamp(0, 1000)) / 1000.
                            }
                            "hair-shade" => {
                                self.state.hair_shade = f64::from(value.clamp(0, 1000)) / 1000.
                            }
                            "hair-style" => {
                                self.state.hair_style = (value.max(0) as usize)
                                    .min(self.state.sex(d).hair_styles.len().saturating_sub(1))
                            }
                            "hair-color" => {
                                self.state.hair_color = (value.max(0) as usize)
                                    .min(self.state.sex(d).hair_colors.len().saturating_sub(1))
                            }
                            "eye-color" => {
                                self.state.eye_color = (value.max(0) as usize)
                                    .min(self.state.sex(d).eye_colors.len().saturating_sub(1))
                            }
                            "heraldry-color" => {
                                self.state.heraldry_color = value.clamp(0, 15) as usize
                            }
                            _ => {}
                        }
                    }
                }
            }
            ControlEvent::Activate(id) => match id.as_str() {
                "login" | "cancel-confirmed" => self.page = "login",
                "cancel" if self.page == "startup" => {
                    actions.push(PanelAction::Host(HostAction::Quit))
                }
                "cancel" => actions.push(PanelAction::Confirm {
                    id: "cancel-creation".into(),
                    text: "Are you sure you want to lose all changes to this character?".into(),
                    accept: vec![PanelAction::Control(ControlEvent::Activate(
                        "cancel-confirmed".into(),
                    ))],
                }),
                "quit" => actions.push(PanelAction::Confirm {
                    id: "quit".into(),
                    text: "\n\nAre you sure you want to leave?\n\n(Default is No)".into(),
                    accept: vec![PanelAction::Host(HostAction::Quit)],
                }),
                "keyboard" => self.page = "keyboard",
                "credits" => {
                    self.page = "credits";
                    self.credits_started = std::time::Instant::now();
                }
                "create" | "quick" => {
                    self.creation_slot = presentation::first_empty_slot(c.pregame);
                    if self.creation_slot.is_none() {
                        self.status = "All character slots are full.".into();
                        return actions;
                    }
                    if let Ok(d) = &self.data {
                        self.heritage_chosen = false;
                        self.sex_chosen = false;
                        self.state = Creation::default();
                        self.state.seed(c.pregame.chargen_seeds.map_or(1, |v| v.1));
                        self.state.constrain(d);
                        self.page = "heritage";
                        if id == "quick" {
                            self.heritage_chosen = true;
                            self.sex_chosen = true;
                            self.state.quick(d);
                            self.page = "name-summary";
                        }
                    }
                }
                "enter" | "delete" | "delete-confirmed" | "restore" => {
                    if let Some(ch) = self.selected.and_then(|i| {
                        presentation::characters(c.pregame)
                            .get(i)
                            .and_then(|r| r.character)
                    }) {
                        let action = match id.as_str() {
                            "enter" if ch.seconds_grace_period == 0 => {
                                Some(CharacterAction::LogOn(ch.id))
                            }
                            "restore" if ch.seconds_grace_period > 0 => {
                                Some(CharacterAction::Restore(ch.id))
                            }
                            "delete-confirmed" if ch.seconds_grace_period == 0 => {
                                Some(CharacterAction::Delete(ch.id))
                            }
                            "delete" if ch.name.eq_ignore_ascii_case(self.delete_name.trim()) => {
                                Some(CharacterAction::Delete(ch.id))
                            }
                            _ => None,
                        };
                        if let Some(a) = action {
                            if id == "delete" {
                                actions.push(PanelAction::Confirm {
                                    id: "delete-character".into(),
                                    text: format!("Delete {}?", ch.name),
                                    accept: vec![PanelAction::Control(ControlEvent::Activate(
                                        "delete-confirmed".into(),
                                    ))],
                                });
                            } else {
                                actions.push(PanelAction::Game(UiRequest::CharacterAction(a)));
                                self.waiting = true;
                                self.set_notice = c.pregame.character_set_notices;
                                if id == "delete-confirmed" {
                                    self.delete_name = DELETE_PLACEHOLDER.into();
                                }
                            }
                        } else if id == "delete" {
                            self.status = "Enter the character's name to delete it.".into();
                        }
                    }
                }
                // Back on the first page leaves the wizard, as Menu does.
                "back" if self.page == "heritage" => actions.push(Self::leave_creation()),
                "back" => self.navigate(-1),
                "creation-menu" => actions.push(Self::leave_creation()),
                "creation-help" => actions.push(PanelAction::Host(HostAction::LegacyHelp(0x32))),
                "next"
                    if self.page == "attributes"
                        && self
                            .data
                            .as_ref()
                            .is_ok_and(|d| self.state.remaining_attributes(d) > 0) =>
                {
                    actions.push(PanelAction::Confirm {
                        id: "unspent-attributes".into(),
                        text: "You have not spent all your Attribute Credits. Continue?".into(),
                        accept: vec![PanelAction::Control(ControlEvent::Activate(
                            "next-confirmed".into(),
                        ))],
                    })
                }
                "next" | "next-confirmed" => self.navigate(1),
                "summary" => self.page = "name-summary",
                "rotate-left" => self.state.rotate(true),
                "rotate-right" => self.state.rotate(false),
                "random" => {
                    if let Ok(d) = &self.data {
                        self.state.randomize(d, self.page);
                        if self.page == "heritage" {
                            self.heritage_chosen = true;
                        }
                        if self.page == "sex" {
                            self.sex_chosen = true;
                        }
                    }
                }
                "train" | "specialize" | "untrain" => {
                    if let Ok(d) = &self.data {
                        if let Some(s) = self
                            .state
                            .selected_skill
                            .and_then(|i| d.skills.get(i).filter(|s| s.chargen != 0))
                        {
                            self.state.skill(
                                d,
                                s.id,
                                match id.as_str() {
                                    "train" => 2,
                                    "specialize" => 3,
                                    _ => 1,
                                },
                            );
                        }
                    }
                }
                "create-submit" | "create-confirmed" => {
                    if let Ok(d) = &self.data {
                        self.state.name = model::format_name(&self.state.name);
                        if self.state.name.trim().is_empty() || self.state.name == "Enter name" {
                            self.status = "Enter a character name.".into();
                        } else if !c.pregame.connected {
                            self.status = "The network is not connected.".into();
                        } else if id == "create-submit" && self.state.remaining_attributes(d) > 0 {
                            actions.push(PanelAction::Confirm{id:"unspent-creation".into(),text:"You have not spent all your Attribute Credits. Create this character anyway?".into(),accept:vec![PanelAction::Control(ControlEvent::Activate("create-confirmed".into()))]});
                        } else {
                            let Some(slot) = self
                                .creation_slot
                                .or_else(|| presentation::first_empty_slot(c.pregame))
                            else {
                                self.status = "All character slots are full.".into();
                                return actions;
                            };
                            actions.push(PanelAction::Host(HostAction::LegacyCharGen(Box::new(
                                LegacyCreation {
                                    result: self.state.result(d, slot),
                                    heraldry_symbol: self.state.heraldry.map_or(-1, i32_from),
                                    heraldry_color: u32_from(self.state.heraldry_color),
                                },
                            ))));
                            self.waiting = true;
                            self.created_name = Some(self.state.name.clone());
                            self.creation_verified = false;
                            self.response_notice = c.pregame.chargen_response_notices;
                            self.set_notice = c.pregame.character_set_notices;
                        }
                    }
                }
                "key-done" => {
                    if c.keyboard.dirty {
                        self.key_transition = Some(KeyTransition::Done);
                        actions.push(PanelAction::Question {
                            id: "save-before-key-exit".into(),
                            text: "\n\nSave current changes?".into(),
                            accept: vec![PanelAction::Control(ControlEvent::Activate(
                                "key-save-transition".into(),
                            ))],
                            reject: vec![
                                PanelAction::Host(HostAction::RestoreBindings),
                                PanelAction::Control(ControlEvent::Activate("key-leave".into())),
                            ],
                        });
                    } else if c.pregame.in_world {
                        actions.push(PanelAction::Close);
                    } else {
                        self.page = "login";
                    }
                }
                "key-leave" => {
                    self.key_transition = None;
                    self.key_saved_transition = false;
                    if c.pregame.in_world {
                        actions.push(PanelAction::Close);
                    } else {
                        self.page = "login";
                    }
                }
                "key-save" | "key-save-transition" => {
                    if id == "key-save" {
                        self.key_transition = None;
                    }
                    self.save_scheme = Some(if c.keyboard.scheme == 0 {
                        String::new()
                    } else {
                        c.keyboard
                            .schemes
                            .get(c.keyboard.scheme as usize)
                            .cloned()
                            .unwrap_or_default()
                    });
                }
                "key-save-cancel" | "key-transition-cancel" => {
                    self.save_scheme = None;
                    self.key_transition = None;
                    self.key_saved_transition = false;
                }
                "key-save-confirm" => {
                    if let Some(name) = self.save_scheme.take() {
                        if !name.trim().is_empty() {
                            self.key_saved_transition = self.key_transition.is_some();
                            if c.keyboard
                                .schemes
                                .iter()
                                .any(|s| s.eq_ignore_ascii_case(&name))
                            {
                                actions.push(PanelAction::Question {
                                    id: "overwrite-key-scheme".into(),
                                    text: "\n\nThat name is in use.  Do you want to overwrite it?"
                                        .into(),
                                    accept: vec![PanelAction::Host(HostAction::OverwriteKeyMap {
                                        name,
                                    })],
                                    reject: vec![PanelAction::Control(ControlEvent::Activate(
                                        "key-transition-cancel".into(),
                                    ))],
                                });
                            } else {
                                actions.push(PanelAction::Host(HostAction::SaveKeyMapAs { name }));
                            }
                        }
                    }
                }
                "key-reset" => actions.push(PanelAction::Confirm {
                    id: "reset-keys".into(),
                    text: "You will lose all the changes made to this scheme. Are you sure?".into(),
                    accept: vec![PanelAction::Host(HostAction::RestoreBindings)],
                }),
                "key-delete" => {
                    if let Some(name) = c.keyboard.schemes.get(c.keyboard.scheme as usize) {
                        actions.push(PanelAction::Confirm {
                            id: "delete-key-scheme".into(),
                            text: format!("Delete keyboard scheme {name}?"),
                            accept: vec![PanelAction::Host(HostAction::DeleteKeyScheme {
                                name: name.clone(),
                            })],
                        });
                    }
                }
                "key-cancel" => {
                    self.page = "keyboard";
                    actions.push(PanelAction::Host(HostAction::CancelBindingCapture));
                }
                "key-capture" | "key-remove" => {
                    if let Some(b) = self.keyboard_row.and_then(|i| c.keyboard.bindings.get(i)) {
                        actions.push(PanelAction::Host(if id == "key-capture" {
                            HostAction::CaptureBinding {
                                action: b.action,
                                map: b.map,
                                slot: self.key_slot,
                            }
                        } else {
                            HostAction::ClearBindingSlot {
                                action: b.action,
                                map: b.map,
                                slot: self.key_slot,
                            }
                        }));
                    }
                }
                _ => {
                    if let Ok(d) = &self.data {
                        for (prefix, kind) in [
                            ("hair-color-pick-", 0),
                            ("eye-color-pick-", 1),
                            ("hair-style-pick-", 2),
                        ] {
                            if let Some(i) = suffix(&id, prefix) {
                                match kind {
                                    0 => self.state.hair_color = i,
                                    1 => self.state.eye_color = i,
                                    _ => self.state.hair_style = i,
                                };
                            }
                        }
                        if let Some((slot, index)) = id
                            .strip_prefix("color-pick-")
                            .and_then(|s| s.split_once('-'))
                        {
                            if let (Ok(slot), Ok(index)) =
                                (slot.parse::<usize>(), index.parse::<usize>())
                            {
                                if slot < 4 && index < self.state.clothing_colors(d, slot).len() {
                                    self.state.colors[slot] = index;
                                }
                            }
                        }
                        if let Some((part, index)) = id
                            .strip_prefix("face-pick-")
                            .and_then(|s| s.split_once('-'))
                        {
                            if let (Ok(part), Ok(index)) =
                                (part.parse::<usize>(), index.parse::<usize>())
                            {
                                if part < 3 {
                                    self.state.face[part] = index;
                                }
                            }
                        }
                        for (prefix, delta) in [("face-prev-", -1), ("face-next-", 1)] {
                            if let Some(value) = id.strip_prefix(prefix) {
                                for part in 0..3 {
                                    if value == "all" || value.parse::<usize>() == Ok(part) {
                                        let n = [
                                            self.state.sex(d).eyes.len(),
                                            self.state.sex(d).noses.len(),
                                            self.state.sex(d).mouths.len(),
                                        ][part];
                                        self.state.face[part] = (i32_from(self.state.face[part])
                                            + delta)
                                            .clamp(0, i32_from(n.saturating_sub(1)))
                                            as usize;
                                    }
                                }
                            }
                        }
                    }
                    for (prefix, delta) in [("skill-up-", 1), ("skill-down-", -1)] {
                        if let Some(i) = suffix(&id, prefix) {
                            if let Ok(d) = &self.data {
                                if let Some(skill) = d.skills.get(i) {
                                    self.state.selected_skill = Some(i);
                                    let level =
                                        self.state.skills.get(&skill.id).copied().unwrap_or(1);
                                    self.state.skill(d, skill.id, level + delta);
                                }
                            }
                        }
                    }
                    if let Some(pair) = id.strip_prefix("binding-").and_then(|v| v.split_once('-'))
                    {
                        if let (Ok(row), Ok(slot)) =
                            (pair.0.parse::<usize>(), pair.1.parse::<usize>())
                        {
                            if let Some(b) = c.keyboard.bindings.get(row) {
                                self.keyboard_row = Some(row);
                                self.capture_before = c.keyboard.capture_revision;
                                self.key_slot = slot;
                                self.page = "key-edit";
                                actions.push(PanelAction::Host(HostAction::CaptureBinding {
                                    action: b.action,
                                    map: b.map,
                                    slot,
                                }));
                            }
                        }
                    }
                    if let Some(i) = suffix(&id, "key-slot-") {
                        if i < 3 {
                            self.key_slot = i;
                        }
                    }
                }
            },
            _ => {}
        }
        actions
    }
}
fn suffix(id: &str, prefix: &str) -> Option<usize> {
    id.strip_prefix(prefix)?.parse().ok()
}
