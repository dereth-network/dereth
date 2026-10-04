//! The barber panel -- the modal opened by `0x0075 Character_StartBarber`.
//!
//! On the start-barber notice the client initializes private
//! character-generation state and a preview view, then shows this shipped subtree. This module
//! connects that start/edit-preview/cancel lifetime, the ordinary-human appearance controls and
//! the exact `0x0311` Apply request. Cancel in the element-message handler only destroys
//! the modal.

use std::rc::Rc;

use dereth_primitives::DataId;
use dereth_ui::{ElemHandle, ElementId, ElementMessage, UiSystem};

use crate::screens::chargen::{
    appearance, set_derived_image, Cg3dView, CharGenTables, EParts, ERotateDirection,
    HUMAN_SETUP_ID, PALETTE_INDEX_EYES, PALETTE_OFFSET_HAIR, PALETTE_OFFSET_SKIN,
    STATE_ROW_SELECTED, STATE_ROW_UNSELECTED,
};
use crate::view::{BarberAppearance, BarberView, GameView, UiRequest};
use dereth_chargen::CharGenState;

/// `<BRBR>`, the barber panel.
pub const PANEL: ElementId = ElementId(0x1000_0598);
/// The character preview's viewport.
pub const VIEWPORT: ElementId = ElementId(0x1000_059B);
/// Hair, eyes, nose, mouth and skin rows, in the native field order.
pub const PART_ROWS: [(ElementId, EParts); 5] = [
    (ElementId(0x1000_059E), EParts::Hair),
    (ElementId(0x1000_059F), EParts::Eyes),
    (ElementId(0x1000_05A0), EParts::Nose),
    (ElementId(0x1000_05A1), EParts::Mouth),
    (ElementId(0x1000_05A2), EParts::Skin),
];
/// The shared previous arrow below each part row.
pub const PREVIOUS: ElementId = ElementId(0x1000_030A);
/// The shared next arrow below each part row.
pub const NEXT: ElementId = ElementId(0x1000_030B);
/// Rotate clockwise.
pub const ROTATE_CLOCKWISE: ElementId = ElementId(0x1000_05A4);
/// Rotate counter-clockwise.
pub const ROTATE_COUNTERCLOCKWISE: ElementId = ElementId(0x1000_05A5);
/// Apply.
pub const APPLY: ElementId = ElementId(0x1000_05A6);
/// Cancel, whose native arm closes without sending.
pub const CANCEL: ElementId = ElementId(0x1000_05A7);
/// The shared special option: hide the crown for Umbraen/Penumbraen, or Earthbound for Empyrean.
pub const OPTION1: ElementId = ElementId(0x1000_05C9);
const OPTION2: ElementId = ElementId(0x1000_05CA);
const OPTION3: ElementId = ElementId(0x1000_05CB);
const CAPTION_STRING_TABLE: DataId = DataId(0x2300_0001);
const SHADOW_NO_CROWN_CAPTION: &str = "ID_Barber_Shadow_NoCrown";
const EMPYREAN_EARTHBOUND_CAPTION: &str = "ID_Barber_Empyrean_Earthbound";
const UNDEAD_NO_FLAME_CAPTION: &str = "ID_Barber_Undead_NoFlame";
const UMBRAEN_MALE_CROWN_SETUP: DataId = DataId(0x0200_196F);
const UMBRAEN_MALE_NO_CROWN_SETUP: DataId = DataId(0x0200_1A5F);
const UMBRAEN_FEMALE_CROWN_SETUP: DataId = DataId(0x0200_1970);
const UMBRAEN_FEMALE_NO_CROWN_SETUP: DataId = DataId(0x0200_1A5E);
const UMBRAEN_MALE_CROWN_EFFECT: DataId = DataId(0x3300_1256);
const UMBRAEN_MALE_NO_CROWN_EFFECT: DataId = DataId(0x3300_12BB);
const UMBRAEN_FEMALE_CROWN_EFFECT: DataId = DataId(0x3300_1257);
const UMBRAEN_FEMALE_NO_CROWN_EFFECT: DataId = DataId(0x3300_12BA);
const PENUMBRAEN_MALE_CROWN_SETUP: DataId = DataId(0x0200_196E);
const PENUMBRAEN_MALE_NO_CROWN_SETUP: DataId = DataId(0x0200_1A5D);
const PENUMBRAEN_FEMALE_CROWN_SETUP: DataId = DataId(0x0200_196D);
const PENUMBRAEN_FEMALE_NO_CROWN_SETUP: DataId = DataId(0x0200_1A5C);
const PENUMBRAEN_MALE_CROWN_EFFECT: DataId = DataId(0x3300_1255);
const PENUMBRAEN_FEMALE_CROWN_EFFECT: DataId = DataId(0x3300_1254);
const EMPYREAN_MALE_FLOAT_MOTION: DataId = DataId(0x0900_020B);
const EMPYREAN_FEMALE_FLOAT_MOTION: DataId = DataId(0x0900_020A);
const EMPYREAN_MALE_EARTHBOUND_MOTION: DataId = DataId(0x0900_020E);
const EMPYREAN_FEMALE_EARTHBOUND_MOTION: DataId = DataId(0x0900_020D);
const UNDEAD_MALE_SKELETON_SETUP: DataId = DataId(0x0200_1A9C);
const UNDEAD_MALE_SKELETON_NO_FLAME_SETUP: DataId = DataId(0x0200_1A9E);
const UNDEAD_MALE_ZOMBIE_SETUP: DataId = DataId(0x0200_1A9D);
const UNDEAD_MALE_ZOMBIE_NO_FLAME_SETUP: DataId = DataId(0x0200_1A96);
const UNDEAD_FEMALE_SKELETON_SETUP: DataId = DataId(0x0200_1AA0);
const UNDEAD_FEMALE_SKELETON_NO_FLAME_SETUP: DataId = DataId(0x0200_1A9F);
const UNDEAD_FEMALE_ZOMBIE_SETUP: DataId = DataId(0x0200_1AA1);
const UNDEAD_FEMALE_ZOMBIE_NO_FLAME_SETUP: DataId = DataId(0x0200_1AA2);
const UNDEAD_MALE_SKELETON_EFFECT: DataId = DataId(0x3300_12D4);
const UNDEAD_MALE_ZOMBIE_EFFECT: DataId = DataId(0x3300_12D3);
const UNDEAD_FEMALE_SKELETON_EFFECT: DataId = DataId(0x3300_12D5);
const UNDEAD_FEMALE_ZOMBIE_EFFECT: DataId = DataId(0x3300_12D6);

/// The barber's private appearance state and viewport.
#[derive(Debug, Default)]
pub struct BarberPanel {
    root: Option<ElemHandle>,
    tables: Option<Rc<CharGenTables>>,
    /// The private character-generation state initialized from the notice.
    pub state: CharGenState,
    /// The private preview view consumed by the host renderer.
    pub view3d: Cg3dView,
    /// Last notice edge adopted. Kept after Cancel so the pull seam cannot reopen it.
    pub active_generation: Option<u64>,
    /// Accepted appearance-arrow gestures, an explicit input denominator.
    pub edits: u64,
    /// Accepted Cancel gestures.
    pub cancels: u64,
    /// Accepted Apply gestures, including native generation failures (which still close).
    pub applies: u64,
    /// The current body part, which gives the shared colour wheel and shade bar their meaning.
    current_part: Option<EParts>,
    /// The current colour index; `None` is native `-1` before the first selection.
    current_color: Option<i32>,
    /// The colour wheel's nine channel-mean colours for the current family.
    color_wheel: [Option<u32>; 9],
    /// The current family's full native colour count; only the first nine have authored spots.
    color_count: usize,
    /// Native option-one state for heritages 5 and 10. Their wire option remains literal zero;
    /// this chooses the sex-specific crown/no-crown setup and local transformation script.
    no_crown: bool,
    /// Native Empyrean option one: Earthbound when checked, floating when unchecked.
    empyrean_earthbound: bool,
    /// Native Undead option one: suppress the setup's flame particles when checked.
    undead_no_flame: bool,
}

impl BarberPanel {
    /// Bind the existing shipped panel; it begins hidden with the rest of the panel stack.
    pub fn post_init(&mut self, ui: &mut UiSystem, gameplay_root: ElemHandle) {
        self.root = ui.get_child_recursive(gameplay_root, PANEL);
        if let Some(root) = self.root {
            ui.set_visible(root, false);
        }
    }

    /// Supply the same two dat tables character generation consumes.
    pub fn set_tables(&mut self, tables: Rc<CharGenTables>) {
        self.tables = Some(tables);
    }

    /// The host-side preview dresser needs the same table handle as character generation.
    #[must_use]
    pub fn tables(&self) -> Option<Rc<CharGenTables>> {
        self.tables.clone()
    }

    /// Pull one new native Start notice into the modal.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let Some(start) = view.barber() else {
            return false;
        };
        if self.active_generation == Some(start.generation) {
            return false;
        }
        self.active_generation = Some(start.generation);
        self.initialize(start);
        if let Some(root) = self.root {
            ui.set_visible(root, true);
        }
        self.configure_special_chrome(ui);
        // Page initialisation ends by selecting the hair part. Without this the
        // authored shared wheel has no part and its inherited visible pointers can cover spots.
        self.set_selection(ui, EParts::Hair);
        true
    }

    fn initialize(&mut self, start: BarberView) {
        self.state = CharGenState::default();
        let setup = DataId(start.setup_id);
        self.no_crown = match start.heritage {
            5 => matches!(
                setup,
                UMBRAEN_MALE_NO_CROWN_SETUP | UMBRAEN_FEMALE_NO_CROWN_SETUP
            ),
            10 => {
                matches!(
                    setup,
                    PENUMBRAEN_MALE_NO_CROWN_SETUP | PENUMBRAEN_FEMALE_NO_CROWN_SETUP
                )
            }
            _ => false,
        };
        // Page initialisation reads argument 15, the incoming `option1`, not argument 14's
        // setup ID.
        self.empyrean_earthbound = start.heritage == 9 && start.option1 != 0;
        self.undead_no_flame = start.heritage == 11
            && matches!(
                setup,
                UNDEAD_MALE_SKELETON_NO_FLAME_SETUP
                    | UNDEAD_MALE_ZOMBIE_NO_FLAME_SETUP
                    | UNDEAD_FEMALE_SKELETON_NO_FLAME_SETUP
                    | UNDEAD_FEMALE_ZOMBIE_NO_FLAME_SETUP
            );
        let Some(tables) = self.tables.clone() else {
            self.state.heritage_group = start.heritage;
            self.state.gender = start.gender;
            self.state.setup_id = DataId(start.setup_id);
            self.initialize_view();
            return;
        };

        // Native initializes the shared char-gen state with a full random character first, then
        // clears headgear and replaces every appearance value represented by this notice.
        self.state
            .randomize_character(&tables.chargen, &tables.skills, true);
        self.state.set_headgear_style(&tables.chargen, -1);
        self.state.set_gender(&tables.chargen, start.gender);
        self.state
            .set_heritage_group(&tables.chargen, &tables.skills, start.heritage);

        if let Some(sx) = self.state.sex(&tables.chargen) {
            self.state.hair_style = find_hair(sx, start.head_object).unwrap_or(0);
            self.state.eyes_strip = find_eyes(sx, start.eyes_texture).unwrap_or(0);
            self.state.nose_strip = find_strip(&sx.nose_strips, start.nose_texture).unwrap_or(0);
            self.state.mouth_strip = find_strip(&sx.mouth_strips, start.mouth_texture).unwrap_or(0);
            self.state.eye_color = sx
                .eye_colors
                .iter()
                .position(|id| *id == start.eyes_palette)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(0);
            if let Some(colors) = tables.colors.as_ref() {
                self.state.skin_shade = inverse_shade(
                    colors.pal_set_palettes(sx.skin_palset).as_deref(),
                    start.skin_palette,
                )
                .unwrap_or(0.0);
                let (hair_color, hair_shade) = sx
                    .hair_colors
                    .iter()
                    .enumerate()
                    .find_map(|(color, set)| {
                        inverse_shade(
                            colors.pal_set_palettes(DataId(*set)).as_deref(),
                            start.hair_palette,
                        )
                        .and_then(|shade| i32::try_from(color).ok().map(|color| (color, shade)))
                    })
                    .unwrap_or((0, 0.0));
                self.state.hair_color = hair_color;
                self.state.hair_shade = hair_shade;
            }
        }
        self.initialize_view();
        let setup = self.state.get_setup_id(&tables.chargen);
        self.view3d.setup = if setup.0 == 0 { HUMAN_SETUP_ID } else { setup };
    }

    /// Page initialisation: Empyrean exposes its localized Earthbound toggle;
    /// Umbraen and Penumbraen expose the localized no-crown toggle. All three lower Apply/Cancel
    /// to make room. Shadow heritages accept either sex's incoming no-crown setup ID, while their
    /// generated setup and local effects remain sex-specific.
    fn configure_special_chrome(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.root else { return };
        let special = matches!(self.state.heritage_group, 5 | 9 | 10 | 11);
        for id in [OPTION1, OPTION2, OPTION3] {
            if let Some(h) = ui.get_child_recursive(root, id) {
                ui.set_visible(h, special && id == OPTION1);
            }
        }
        if let Some(h) = ui.get_child_recursive(root, OPTION1) {
            let selected = match self.state.heritage_group {
                9 => self.empyrean_earthbound,
                11 => self.undead_no_flame,
                _ => self.no_crown,
            };
            ui.set_state(h, dereth_ui::StateId(if selected { 6 } else { 1 }));
            if special {
                let token = match self.state.heritage_group {
                    9 => EMPYREAN_EARTHBOUND_CAPTION,
                    11 => UNDEAD_NO_FLAME_CAPTION,
                    _ => SHADOW_NO_CROWN_CAPTION,
                };
                let caption = ui
                    .resolve_string(
                        CAPTION_STRING_TABLE,
                        dereth_primitives::num::hash::str_hash(token.as_bytes()),
                    )
                    .unwrap_or_else(|| token.to_owned());
                if let Some(text) = ui.text_element_mut(h) {
                    text.set_text(&caption);
                }
            }
        }
        let y = if special { 0x12A } else { 0x112 };
        for (id, x) in [(APPLY, 0x160), (CANCEL, 0xE8)] {
            if let Some(h) = ui.get_child_recursive(root, id) {
                ui.move_to(h, x, y);
            }
        }
        if matches!(self.state.heritage_group, 5 | 10 | 11) {
            self.update_special_setup();
        }
    }

    fn initialize_view(&mut self) {
        self.view3d = Cg3dView::default();
        self.view3d.initialize(&self.state);
        self.view3d.viewport = Some(VIEWPORT);
        // Page initialisation: the initial camera, with the closer special-
        // creature position used by Gear Knight and both Olthoi heritages.
        self.view3d.camera_position = match self.state.heritage_group {
            6 | 12 | 13 => [0.0, -0.8, 1.5],
            _ => [0.0, -0.65, 1.7],
        };
        self.view3d.camera_direction = [0.0, 0.0, 0.0];
        self.view3d.zoomed_in = true;
        self.view3d.animating = false;
        self.view3d.bg_setup = DataId(0);
    }

    /// The actual shipped arrow/rotation/Cancel controls.
    pub fn on_element_message(&mut self, ui: &mut UiSystem, m: &ElementMessage) {
        if !self.is_open(ui) {
            return;
        }
        let Some(id) = ui.node(m.source).map(dereth_ui::ElementNode::element_id) else {
            return;
        };
        if m.id == dereth_ui::msg::element::id::SCROLL_POSITION && id == appearance::SHADE_SCROLL {
            // In the element-message handler: the scrollbar reports integer thousandths.
            let shade = (f64::from(m.p1) * 0.001).clamp(0.0, 1.0);
            if self.set_shade(ui, shade) {
                self.edits += 1;
            }
            return;
        }
        if m.id != dereth_ui::msg::element::id::BUTTON_CLICKED {
            return;
        }
        if let Some((_, part)) = PART_ROWS.iter().find(|(row, _)| *row == id) {
            self.set_selection(ui, *part);
            return;
        }
        if let Some(index) = appearance::COLOR_SPOTS.iter().position(|spot| *spot == id) {
            if self.set_color(ui, i32::try_from(index).unwrap_or(0)) {
                self.edits += 1;
            }
            return;
        }
        match id {
            CANCEL => {
                self.close(ui);
                self.cancels += 1;
            }
            APPLY => {
                // Retail closes even when base-appearance generation fails; only the successful
                // arm sends the finish-barber request ([`UiRequest::BarberFinish`]). Empyrean installs its selected motion table;
                // heritages 5 and 10 replace local particles, including the zero-PES arm. These
                // are native local preludes; the authoritative object-description update owns
                // the lasting appearance.
                if self.state.heritage_group == 9 {
                    ui.requests.emit(UiRequest::BarberLocalMotionTable(
                        self.empyrean_motion_table(),
                    ));
                }
                if self.state.heritage_group == 11 {
                    if let Some((_, effect)) = self.undead_setup_and_effect() {
                        ui.requests.emit(UiRequest::BarberLocalEffect(effect));
                    }
                }
                if matches!(self.state.heritage_group, 5 | 10) {
                    if let Some((_, effect)) = self.special_setup_and_effect() {
                        ui.requests.emit(UiRequest::BarberLocalEffect(effect));
                    }
                }
                if let Some(appearance) = self.finish_appearance() {
                    ui.requests.emit(UiRequest::BarberFinish(appearance));
                }
                self.close(ui);
                self.applies += 1;
            }
            ROTATE_CLOCKWISE => self.view3d.rotate(ERotateDirection::Clockwise),
            ROTATE_COUNTERCLOCKWISE => self.view3d.rotate(ERotateDirection::CounterClockwise),
            OPTION1 if matches!(self.state.heritage_group, 5 | 9 | 10 | 11) => {
                let selected = ui
                    .node(m.source)
                    .is_some_and(|node| node.state == dereth_ui::StateId(6));
                if self.state.heritage_group == 9 {
                    self.empyrean_earthbound = selected;
                } else if self.state.heritage_group == 11 {
                    self.undead_no_flame = selected;
                    self.update_special_setup();
                } else {
                    self.no_crown = selected;
                    self.update_special_setup();
                }
                self.edits += 1;
            }
            PREVIOUS | NEXT => {
                let Some(part) = row_part(ui, m.source) else {
                    return;
                };
                let delta = if id == NEXT { 1 } else { -1 };
                if self.cycle(part, delta) {
                    self.edits += 1;
                    // Native arrows end by selecting their row's part, so the shared wheel tracks
                    // whichever appearance family the player just changed.
                    self.set_selection(ui, part);
                }
            }
            _ => {}
        }
    }

    /// Rotate the private preview on the same global tick as character generation.
    pub fn tick_preview(&mut self, dt: f64) {
        self.view3d.do_rotation(dt);
    }

    #[must_use]
    pub fn is_open(&self, ui: &UiSystem) -> bool {
        self.root
            .and_then(|h| ui.node(h))
            .is_some_and(|n| n.region.flags.visible)
    }

    fn close(&mut self, ui: &mut UiSystem) {
        if let Some(root) = self.root {
            ui.set_visible(root, false);
        }
        self.view3d.viewport = None;
    }

    /// Set the selected part. Colour-button ordinals are interpreted through this
    /// selected family; they are not texture-part indices.
    fn set_selection(&mut self, ui: &mut UiSystem, part: EParts) {
        let Some(tables) = self.tables.clone() else {
            return;
        };
        let Some(sx) = self.state.sex(&tables.chargen) else {
            return;
        };
        let (color, colors, shade) = match part {
            EParts::Hair => (
                self.state.hair_color,
                sx.hair_colors.len(),
                Some(self.state.hair_shade),
            ),
            EParts::Eyes => (self.state.eye_color, sx.eye_colors.len(), None),
            EParts::Nose | EParts::Mouth | EParts::Skin => (0, 1, Some(self.state.skin_shade)),
            EParts::Invalid
            | EParts::Headgear
            | EParts::Shirt
            | EParts::Trousers
            | EParts::Footwear => return,
        };
        let Some(root) = self.root else { return };

        for (row, state) in [
            (self.current_part.and_then(part_row), STATE_ROW_UNSELECTED),
            (part_row(part), STATE_ROW_SELECTED),
        ] {
            let Some(id) = row else { continue };
            if let Some(h) = ui.get_child_recursive(root, id) {
                ui.set_state(h, state);
            }
        }
        for (spot, pointer) in appearance::COLOR_SPOTS
            .iter()
            .zip(appearance::COLOR_POINTERS.iter())
        {
            if let Some(h) = ui.get_child_recursive(root, *spot) {
                ui.set_visible(h, true);
            }
            if let Some(h) = ui.get_child_recursive(root, *pointer) {
                ui.set_visible(h, false);
            }
        }
        if let Some(h) = ui.get_child_recursive(root, appearance::SHADE_SCROLL) {
            ui.set_visible(h, shade.is_some());
        }
        self.current_part = Some(part);
        self.fill_color_wheel(part);
        self.do_color_spots(ui);
        self.current_color = None;
        if usize::try_from(color).ok().is_some_and(|i| i < colors) {
            let _ = self.set_color(ui, color);
        }
        if let Some(shade) = shade {
            let _ = self.set_shade(ui, shade);
        }
    }

    /// Set the colour: only Hair and Eyes own palette-list choices.
    fn set_color(&mut self, ui: &mut UiSystem, color: i32) -> bool {
        let Some(tables) = self.tables.clone() else {
            return false;
        };
        let Some(sx) = self.state.sex(&tables.chargen) else {
            return false;
        };
        let count = match self.current_part {
            Some(EParts::Hair) => sx.hair_colors.len(),
            Some(EParts::Eyes) => sx.eye_colors.len(),
            Some(EParts::Nose | EParts::Mouth | EParts::Skin) => 1,
            _ => return false,
        };
        let Some(index) = usize::try_from(color).ok().filter(|i| *i < count) else {
            return false;
        };
        if let Some(root) = self.root {
            for (old, visible) in [(self.current_color, false), (Some(color), true)] {
                let Some(id) = old
                    .and_then(|i| usize::try_from(i).ok())
                    .and_then(|i| appearance::COLOR_POINTERS.get(i).copied())
                else {
                    continue;
                };
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_visible(h, visible);
                }
            }
        }
        self.current_color = Some(color);
        match self.current_part {
            Some(EParts::Hair) => self.state.set_hair_color(i32::try_from(index).unwrap_or(0)),
            Some(EParts::Eyes) => self.state.set_eye_color(i32::try_from(index).unwrap_or(0)),
            _ => {}
        }
        self.do_grad_disk(ui, self.current_part == Some(EParts::Eyes));
        true
    }

    /// Set the shade: Hair has its own shade; Nose, Mouth and Skin all share
    /// the skin shade. Eyes has no shade arm and hides this control.
    fn set_shade(&mut self, ui: &mut UiSystem, shade: f64) -> bool {
        let shade = shade.clamp(0.0, 1.0);
        match self.current_part {
            Some(EParts::Hair) => self.state.set_hair_shade(shade),
            Some(EParts::Nose | EParts::Mouth | EParts::Skin) => self.state.set_skin_shade(shade),
            _ => return false,
        }
        if let Some(root) = self.root {
            if let Some(h) = ui.get_child_recursive(root, appearance::SHADE_SCROLL) {
                #[allow(clippy::cast_possible_truncation)]
                ui.set_attribute_float(
                    h,
                    dereth_ui::widgets::scrollbar::attr::POSITION,
                    shade as f32,
                );
            }
        }
        true
    }

    /// The selection's palette reads. Hair and skin average every Palette
    /// in a PalSet channel-by-channel; Eyes reads entry `0x103` from each Palette directly.
    fn fill_color_wheel(&mut self, part: EParts) {
        self.color_wheel = [None; 9];
        self.color_count = 0;
        let Some(tables) = self.tables.clone() else {
            return;
        };
        let Some(colors) = tables.colors.clone() else {
            return;
        };
        let Some(sx) = self.state.sex(&tables.chargen) else {
            return;
        };
        let values: Vec<Option<u32>> = match part {
            EParts::Hair => sx
                .hair_colors
                .iter()
                .map(|id| colors.pal_set_color(DataId(*id), PALETTE_OFFSET_HAIR))
                .collect(),
            EParts::Eyes => sx
                .eye_colors
                .iter()
                .map(|id| colors.palette_color(DataId(*id), PALETTE_INDEX_EYES))
                .collect(),
            EParts::Nose | EParts::Mouth | EParts::Skin => {
                vec![colors.pal_set_color(sx.skin_palset, PALETTE_OFFSET_SKIN)]
            }
            EParts::Invalid
            | EParts::Headgear
            | EParts::Shirt
            | EParts::Trousers
            | EParts::Footwear => Vec::new(),
        };
        self.color_count = values.len();
        for (slot, color) in self.color_wheel.iter_mut().zip(values) {
            *slot = color;
        }
    }

    /// Draw the colour spots: derive each visible bullet by exact black
    /// replacement, and put the shipped ColorEmpty image in every slot past the colour count.
    fn do_color_spots(&self, ui: &mut UiSystem) {
        let Some(root) = self.root else { return };
        let Some(tables) = self.tables.as_ref() else {
            return;
        };
        let art = tables.color_wheel_art;
        for (i, id) in appearance::COLOR_SPOTS.iter().enumerate() {
            let Some(h) = ui.get_child_recursive(root, *id) else {
                continue;
            };
            let (did, op) = match (i < self.color_count, self.color_wheel[i]) {
                (true, Some(color)) => (
                    art.bullet,
                    Some(dereth_ui::region::SurfaceOp::ReplaceColor {
                        from: dereth_ui::region::SurfaceOp::OPAQUE_BLACK,
                        to: color,
                    }),
                ),
                _ => (art.empty, None),
            };
            set_derived_image(ui, h, did, op);
        }
    }

    /// Draw the gradient disk: Eyes uses the flat plug; all shaded families
    /// multiply ColorRing by the currently selected spot colour.
    fn do_grad_disk(&self, ui: &mut UiSystem, eyes: bool) {
        let Some(root) = self.root else { return };
        let Some(tables) = self.tables.as_ref() else {
            return;
        };
        let Some(h) = ui.get_child_recursive(root, appearance::GRAD_CIRCLE) else {
            return;
        };
        if eyes {
            set_derived_image(ui, h, tables.color_wheel_art.plug, None);
            return;
        }
        let color = self
            .current_color
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.color_wheel.get(i).copied().flatten());
        set_derived_image(
            ui,
            h,
            tables.color_wheel_art.ring,
            color.map(dereth_ui::region::SurfaceOp::Multiply),
        );
    }

    /// The char-gen base-appearance generation, reduced to the sixteen fields
    /// the finish-barber request consumes. The object-description merge remains the preview dresser's
    /// job; this extracts the same part/texture changes and concrete palette IDs from its inputs.
    fn finish_appearance(&self) -> Option<BarberAppearance> {
        let tables = self.tables.as_ref()?;
        let sx = self.state.sex(&tables.chargen)?;
        let pick = |i: i32, n: usize| usize::try_from(i).ok().filter(|i| *i < n);
        let hair = &sx.hair_styles[pick(self.state.hair_style, sx.hair_styles.len())?];
        let eyes = &sx.eye_strips[pick(self.state.eyes_strip, sx.eye_strips.len())?];
        let nose = &sx.nose_strips[pick(self.state.nose_strip, sx.nose_strips.len())?].1;
        let mouth = &sx.mouth_strips[pick(self.state.mouth_strip, sx.mouth_strips.len())?].1;

        let head_object = if self.state.heritage_group == 6 {
            0
        } else {
            hair.objdesc
                .anim_part_changes
                .first()
                .map_or(0, |(_, id)| id.0)
        };
        // Native omits the hair texture pair entirely for Gear Knight and both Olthoi heritages.
        let head = if matches!(self.state.heritage_group, 6 | 12 | 13) {
            (0, 0)
        } else {
            texture_pair(&hair.objdesc, 0)
        };
        let eyes_desc = if hair.bald == 0 {
            &eyes.objdesc
        } else {
            &eyes.objdesc_bald
        };
        let eyes = texture_pair(eyes_desc, 0);
        let nose = texture_pair(nose, 0);
        let mouth = texture_pair(mouth, 0);

        let colors = tables.colors.as_ref()?;
        let skin_palette = palette_at_shade(
            colors.pal_set_palettes(sx.skin_palset)?.as_slice(),
            self.state.skin_shade,
        )?;
        let hair_color = pick(self.state.hair_color, sx.hair_colors.len())?;
        let hair_palette = palette_at_shade(
            colors
                .pal_set_palettes(DataId(sx.hair_colors[hair_color]))?
                .as_slice(),
            self.state.hair_shade,
        )?;
        let eyes_palette = sx.eye_colors[pick(self.state.eye_color, sx.eye_colors.len())?];
        let setup = match self.state.heritage_group {
            5 | 10 => self.special_setup_and_effect()?.0,
            11 => self.undead_setup_and_effect()?.0,
            _ => self.state.get_setup_id(&tables.chargen),
        };

        Some(BarberAppearance {
            base_palette: sx.base_palette.0,
            head_object,
            head_texture: head.1,
            default_head_texture: head.0,
            eyes_texture: eyes.1,
            default_eyes_texture: eyes.0,
            nose_texture: nose.1,
            default_nose_texture: nose.0,
            mouth_texture: mouth.1,
            default_mouth_texture: mouth.0,
            skin_palette,
            hair_palette,
            eyes_palette,
            setup_id: setup.0,
            option1: i32::from(self.state.heritage_group == 9 && self.empyrean_earthbound),
            option2: 0,
        })
    }

    fn special_setup_and_effect(&self) -> Option<(DataId, DataId)> {
        let female = self.state.gender == 2;
        let pair = match (self.state.heritage_group, female, self.no_crown) {
            (5, false, false) => (UMBRAEN_MALE_CROWN_SETUP, UMBRAEN_MALE_CROWN_EFFECT),
            (5, false, true) => (UMBRAEN_MALE_NO_CROWN_SETUP, UMBRAEN_MALE_NO_CROWN_EFFECT),
            (5, true, false) => (UMBRAEN_FEMALE_CROWN_SETUP, UMBRAEN_FEMALE_CROWN_EFFECT),
            (5, true, true) => (
                UMBRAEN_FEMALE_NO_CROWN_SETUP,
                UMBRAEN_FEMALE_NO_CROWN_EFFECT,
            ),
            (10, false, false) => (PENUMBRAEN_MALE_CROWN_SETUP, PENUMBRAEN_MALE_CROWN_EFFECT),
            (10, false, true) => (PENUMBRAEN_MALE_NO_CROWN_SETUP, DataId(0)),
            (10, true, false) => (
                PENUMBRAEN_FEMALE_CROWN_SETUP,
                PENUMBRAEN_FEMALE_CROWN_EFFECT,
            ),
            (10, true, true) => (PENUMBRAEN_FEMALE_NO_CROWN_SETUP, DataId(0)),
            _ => return None,
        };
        Some(pair)
    }

    fn update_special_setup(&mut self) {
        let special = if self.state.heritage_group == 11 {
            self.undead_setup_and_effect()
        } else {
            self.special_setup_and_effect()
        };
        if let Some((setup, _)) = special {
            self.view3d.setup = setup;
        }
    }

    fn undead_setup_and_effect(&self) -> Option<(DataId, DataId)> {
        let tables = self.tables.as_ref()?;
        let generated = self.state.get_setup_id(&tables.chargen);
        // The setup dispatch recognizes the four zombie IDs; every other generated setup takes
        // the skeleton arm. There is no corresponding known-skeleton guard.
        let zombie = matches!(
            generated,
            UNDEAD_MALE_ZOMBIE_SETUP
                | UNDEAD_MALE_ZOMBIE_NO_FLAME_SETUP
                | UNDEAD_FEMALE_ZOMBIE_SETUP
                | UNDEAD_FEMALE_ZOMBIE_NO_FLAME_SETUP
        );
        let female = self.state.gender == 2;
        Some(match (female, zombie, self.undead_no_flame) {
            (false, false, false) => (UNDEAD_MALE_SKELETON_SETUP, UNDEAD_MALE_SKELETON_EFFECT),
            (false, true, false) => (UNDEAD_MALE_ZOMBIE_SETUP, UNDEAD_MALE_ZOMBIE_EFFECT),
            (true, false, false) => (UNDEAD_FEMALE_SKELETON_SETUP, UNDEAD_FEMALE_SKELETON_EFFECT),
            (true, true, false) => (UNDEAD_FEMALE_ZOMBIE_SETUP, UNDEAD_FEMALE_ZOMBIE_EFFECT),
            (false, false, true) => (UNDEAD_MALE_SKELETON_NO_FLAME_SETUP, DataId(0)),
            (false, true, true) => (UNDEAD_MALE_ZOMBIE_NO_FLAME_SETUP, DataId(0)),
            (true, false, true) => (UNDEAD_FEMALE_SKELETON_NO_FLAME_SETUP, DataId(0)),
            (true, true, true) => (UNDEAD_FEMALE_ZOMBIE_NO_FLAME_SETUP, DataId(0)),
        })
    }

    fn empyrean_motion_table(&self) -> DataId {
        match (self.state.gender == 2, self.empyrean_earthbound) {
            (false, false) => EMPYREAN_MALE_FLOAT_MOTION,
            (true, false) => EMPYREAN_FEMALE_FLOAT_MOTION,
            (false, true) => EMPYREAN_MALE_EARTHBOUND_MOTION,
            (true, true) => EMPYREAN_FEMALE_EARTHBOUND_MOTION,
        }
    }

    fn cycle(&mut self, part: EParts, delta: i32) -> bool {
        let Some(tables) = self.tables.clone() else {
            return false;
        };
        let Some(sx) = self.state.sex(&tables.chargen) else {
            return false;
        };
        let (current, count) = match part {
            EParts::Hair => (self.state.hair_style, sx.hair_styles.len()),
            EParts::Eyes => (self.state.eyes_strip, sx.eye_strips.len()),
            EParts::Nose => (self.state.nose_strip, sx.nose_strips.len()),
            EParts::Mouth => (self.state.mouth_strip, sx.mouth_strips.len()),
            EParts::Skin
            | EParts::Invalid
            | EParts::Headgear
            | EParts::Shirt
            | EParts::Trousers
            | EParts::Footwear => return false,
        };
        let Ok(count) = i32::try_from(count) else {
            return false;
        };
        if count == 0 {
            return false;
        }
        let next = (current + delta).rem_euclid(count);
        match part {
            EParts::Hair => self.state.hair_style = next,
            EParts::Eyes => self.state.eyes_strip = next,
            EParts::Nose => self.state.nose_strip = next,
            EParts::Mouth => self.state.mouth_strip = next,
            _ => return false,
        }
        match self.state.heritage_group {
            // The barber panel leaves the special alternate setup latched across Hair/Next. The hair
            // choice changes, but neither that common case nor selection writes
            // the alternate setup id; option gestures and Apply remain the explicit remap points.
            5 | 10 | 11 => {}
            _ => {
                let setup = self.state.get_setup_id(&tables.chargen);
                self.view3d.setup = if setup.0 == 0 { HUMAN_SETUP_ID } else { setup };
            }
        }
        true
    }
}

fn texture_pair(desc: &dereth_assets::tables::ObjDesc, index: usize) -> (u32, u32) {
    desc.texture_changes
        .get(index)
        .map_or((0, 0), |(_, old, new)| (old.0, new.0))
}

fn palette_at_shade(palettes: &[DataId], shade: f64) -> Option<u32> {
    if palettes.is_empty() || !(0.0..=1.0).contains(&shade) {
        return None;
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let index = ((palettes.len() as f64 - 0.000_001) * shade) as usize;
    palettes.get(index).map(|id| id.0)
}

fn row_part(ui: &UiSystem, mut h: ElemHandle) -> Option<EParts> {
    loop {
        let id = ui.node(h)?.element_id();
        if let Some((_, part)) = PART_ROWS.iter().find(|(row, _)| *row == id) {
            return Some(*part);
        }
        h = ui.parent(h)?;
    }
}

fn part_row(part: EParts) -> Option<ElementId> {
    PART_ROWS
        .iter()
        .find_map(|(id, p)| (*p == part).then_some(*id))
}

fn find_hair(sx: &dereth_assets::tables::SexCg, id: u32) -> Option<i32> {
    sx.hair_styles
        .iter()
        .position(|h| {
            h.objdesc
                .anim_part_changes
                .first()
                .is_some_and(|(_, part)| part.0 == id)
        })
        .and_then(|i| i32::try_from(i).ok())
}

fn find_eyes(sx: &dereth_assets::tables::SexCg, id: u32) -> Option<i32> {
    sx.eye_strips
        .iter()
        .position(|e| {
            [&e.objdesc, &e.objdesc_bald]
                .into_iter()
                .any(|d| d.texture_changes.iter().any(|(_, _, new)| new.0 == id))
        })
        .and_then(|i| i32::try_from(i).ok())
}

fn find_strip(strips: &[(u32, dereth_assets::tables::ObjDesc)], id: u32) -> Option<i32> {
    strips
        .iter()
        .position(|(_, d)| d.texture_changes.iter().any(|(_, _, new)| new.0 == id))
        .and_then(|i| i32::try_from(i).ok())
}

fn inverse_shade(palettes: Option<&[DataId]>, palette: u32) -> Option<f64> {
    let palettes = palettes?;
    let index = palettes.iter().position(|id| id.0 == palette)?;
    let numerator = u32::try_from(index).ok()?.checked_add(1)?;
    let denominator = u32::try_from(palettes.len()).ok().filter(|n| *n != 0)?;
    Some(f64::from(numerator) / f64::from(denominator))
}
