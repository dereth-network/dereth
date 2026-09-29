//! Shared element-tree, input and screen fixtures over the retail DATs.

use dereth_input::spec::{activation, ControlCode, SubControlIndex};
use dereth_input::{ControlChord, InputManager};
use dereth_primitives::{AssetSource, DataId};
use dereth_ui::framework::Screen;
use dereth_ui::{ElemHandle, UiSystem};
use dereth_ui_screens::panels::examination::WINDOW;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

pub(crate) fn walk(ui: &UiSystem, h: ElemHandle, out: &mut Vec<ElemHandle>) {
    out.push(h);
    for c in ui.children(h) {
        walk(ui, c, out);
    }
}

pub(crate) fn ty_of(ui: &UiSystem, h: ElemHandle) -> u32 {
    ui.node(h).expect("alive").desc.ty.0
}

pub(crate) fn id_of(ui: &UiSystem, h: ElemHandle) -> u32 {
    ui.node(h).expect("alive").desc.element_id.0
}

/// Put a lamp in a lit state exactly the way its own update does — a state set.
/// Four of the six rest in `0x0D`, which is `Button`'s **disabled** attribute.
pub(crate) fn light(ui: &mut UiSystem, h: ElemHandle) {
    ui.set_state(h, dereth_ui::StateId(1));
}

pub(crate) fn placed(ui: &mut UiSystem, h: ElemHandle) -> (String, usize, i32) {
    let t = ui
        .text_element_mut(h)
        .expect("a CheckboxOption is a TextElement");
    (
        t.glyphs.inq_text(false),
        t.glyphs.glyphs.len(),
        t.glyphs.glyphs.iter().map(|g| g.width).sum(),
    )
}

pub(crate) fn checked(ui: &UiSystem, h: ElemHandle) -> bool {
    dereth_ui_screens::bind::attr_bool(ui, h, ATTR_CHECKED).unwrap_or(false)
}

pub(crate) const ATTR_CHECKED: u32 = 0x0E;

pub(crate) fn manager() -> InputManager {
    let store = dereth_dat::testing::open_store_or_fail();
    let read = |id: DataId| {
        store
            .read(id)
            .unwrap_or_else(|e| panic!("{:#010X} from the retail dats: {e}", id.0))
    };
    let am = read(ACTIONMAP);
    let gm = read(KEYMAP_GM);
    let dm = read(KEYMAP_DEFAULT);
    let mut m = InputManager::on_startup(&am, &dm).expect("on_startup");
    m.init_keymap(None, &gm, &dm).expect("init_keymap");
    m.keymap.devices = dereth_input::keymap::MasterInputMap::read(&gm)
        .expect("gm")
        .devices;
    m.has_focus = true;
    m
}

pub(crate) fn keyboard(offset: u16) -> ControlChord {
    ControlChord::new(
        ControlCode::new(0, SubControlIndex::None, offset),
        0,
        activation::CLICK,
    )
}

/// The shipped `ActionMap`.
pub(crate) const ACTIONMAP: DataId = DataId(0x2600_0000);

/// The default key map.
pub(crate) const KEYMAP_GM: DataId = DataId(0x1400_0000);

/// `DefaultMap`.
pub(crate) const KEYMAP_DEFAULT: DataId = DataId(0x1400_0002);

pub(crate) fn make(class: &str) -> Box<dyn Screen> {
    match class {
        "DataPatchScreen" => {
            dereth_ui_screens::screens::datapatch::DataPatchScreen::create_screen()
        }
        "IntroScreen" => dereth_ui_screens::screens::intro::IntroScreen::create_screen(),
        "CharacterManagementScreen" => {
            dereth_ui_screens::screens::charmgmt::CharacterManagementScreen::create_screen()
        }
        "GamePlayScreen" => dereth_ui_screens::screens::gameplay::GamePlayScreen::create_screen(),
        "EpilogueScreen" => dereth_ui_screens::screens::epilogue::EpilogueScreen::create_screen(),
        "DisconnectedScreen" => {
            dereth_ui_screens::screens::disconnected::DisconnectedScreen::create_screen()
        }
        "CharGenScreen" => dereth_ui_screens::screens::chargen::CharGenScreen::create_screen(),
        "CreditsScreen" => dereth_ui_screens::screens::credits::CreditsScreen::create_screen(),
        other => panic!("no factory for {other}"),
    }
}

pub(crate) fn window_of(ui: &UiSystem, s: &GamePlayScreen) -> ElemHandle {
    let root = *s.roots().first().expect("root");
    ui.get_child_recursive(root, WINDOW)
        .expect("<EXAM> 0x100005F7")
}

pub(crate) fn is_open(ui: &UiSystem, h: ElemHandle) -> bool {
    ui.node(h).expect("node").region.flags.visible
}

pub(crate) mod layout;

pub(crate) mod widget_fixture;
