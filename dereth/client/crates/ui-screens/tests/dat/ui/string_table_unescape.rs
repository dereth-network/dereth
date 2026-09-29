//! The shipped character sheet stores literal backslash-n; the string lookup unescapes it before
//! the pane, no backslash is drawn, and the unescaped sheet overflows the pane.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use crate::common::layout::Strings;
use std::collections::BTreeSet;
use std::rc::Rc;

use dereth_primitives::{AssetSource, DataId, LocalTime, ObjectId};
use dereth_ui::{Delivery, ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::panels::characterinfo;
use dereth_ui_screens::panels::remaining::RemainingPanels;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{CharacterInfo, GameView};

/// `BurdenIndicator` — action `0x10000005`, which opens the character info panel.
const BURDEN_LAMP: ElementId = ElementId(0x1000_00F7);

const PLAYER: ObjectId = ObjectId(0x5000_0001);

/// `panels::statmgmt::STRING_TABLE` — the table every `ID_*` token on the sheet lives in.
const STRING_TABLE: DataId = DataId(0x2300_0001);

fn env() -> UiSystem {
    let (mut ui, _flow, store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui.strings = Some(Rc::new(Strings(Rc::clone(&store))));
    ui
}

fn pump(
    ui: &mut UiSystem,
    s: &mut GamePlayScreen,
    panels: &mut RemainingPanels,
    view: &dyn GameView,
) {
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        let queued = s.take_panel_messages();
        if batch.is_empty() && queued.is_empty() {
            return;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
        for m in queued {
            panels.on_element_message(ui, &m, view);
        }
    }
}

fn screen(view: &dyn GameView) -> (UiSystem, GamePlayScreen, RemainingPanels) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    let mut panels = RemainingPanels::default();
    let root = s.root().expect("the gameplay root");
    panels.post_init(&mut ui, root);
    pump(&mut ui, &mut s, &mut panels, view);
    ui.requests.clear();
    (ui, s, panels)
}

fn find(ui: &UiSystem, s: &GamePlayScreen, id: ElementId) -> ElemHandle {
    let root = s.root().expect("the gameplay root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{:#010X} is not in the shipped tree", id.0))
}

fn centre(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let b = ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// The three events a player produces, and nothing else.
fn click(
    ui: &mut UiSystem,
    s: &mut GamePlayScreen,
    panels: &mut RemainingPanels,
    view: &dyn GameView,
    at: (i32, i32),
) {
    ui.mouse_move(LocalTime(0.0), at.0, at.1);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1, false);
    pump(ui, s, panels, view);
}

#[derive(Debug, Default)]
struct Burdened;

impl GameView for Burdened {
    fn player(&self) -> Option<ObjectId> {
        Some(PLAYER)
    }
    fn character_info(&self) -> Option<CharacterInfo> {
        Some(CharacterInfo {
            innate: [290, 285, 275, 260, 190, 180],
            chess_rank: 1400,
            fishing_skill: 0,
            num_deaths: 7,
            strength: 290,
            endurance: 285,
            load: 1.4,
            encumbrance: 61_000,
            capacity: 43_500,
            augmentations: 3,
            ..CharacterInfo::default()
        })
    }
}

fn open_character_info(
    ui: &mut UiSystem,
    s: &mut GamePlayScreen,
    panels: &mut RemainingPanels,
    view: &dyn GameView,
) -> ElemHandle {
    let lamp = find(ui, s, BURDEN_LAMP);
    ui.set_state(lamp, dereth_ui::StateId(1));
    let at = centre(ui, lamp);
    click(ui, s, panels, view, at);
    panels.update(ui, view);
    find(ui, s, characterinfo::INFO_TEXT)
}

/// Every glyph the pane composes, in order — including the ones below the fold, because the
/// question is what the text *is*, not what is scrolled into view.
fn composed(ui: &mut UiSystem, h: ElemHandle) -> Vec<dereth_ui::text::PlacedGlyph> {
    let box_ = ui.screen_box(h);
    ui.text_element_mut(h)
        .expect("a text element")
        .compose(box_)
}

/// How many lines the composed text occupies, **blank ones included**.
///
/// Distinct pen `y` alone undercounts: the section separator on this sheet is an empty line, which
/// occupies a line box and places no glyph. The line pitch is the smallest gap between two
/// occupied lines, so the count is the full span divided by it — which is what `wrap` produced and
/// what would be handed.
fn line_count(g: &[dereth_ui::text::PlacedGlyph]) -> i32 {
    let (first, last, pitch) = span(g);
    (last - first) / pitch + 1
}

/// The height the composed text occupies, in pixels — the content extent
/// the scrollbar's size update compares against the view.
fn extent(g: &[dereth_ui::text::PlacedGlyph]) -> i32 {
    let (first, last, pitch) = span(g);
    last - first + pitch
}

/// The first occupied line's y, the last one's, and the line pitch. Panics rather than returning a
/// plausible zero when nothing composed: an instrument that cannot look must not report a number.
fn span(g: &[dereth_ui::text::PlacedGlyph]) -> (i32, i32, i32) {
    let ys: Vec<i32> = g
        .iter()
        .map(|p| p.y)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    assert!(
        !ys.is_empty(),
        "nothing composed at all -- this measurement would be of the harness"
    );
    let pitch = ys.windows(2).map(|w| w[1] - w[0]).min().unwrap_or(1).max(1);
    (ys[0], ys[ys.len() - 1], pitch)
}

// =============================================================================================
// 1. Calibration — the escape really is in the shipped data
// =============================================================================================

/// **The instrument's calibration.** If the shipped table did not hold literal backslash-`n` there
/// would be nothing for an unescape to do, and every assertion below would be measuring the
/// harness. Read straight off `client_local_English.dat`, past every resolver.
#[test]
fn the_shipped_character_sheet_stores_its_breaks_as_a_literal_backslash_n() {
    use dereth_assets::Decode;
    let dir = dereth_dat::testing::dat_dir();
    let store = dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail dats open");
    let b = store.read(STRING_TABLE).expect("string table 0x23000001");
    let t = dereth_assets::ui::StringTable::decode_payload(STRING_TABLE, &b).expect("it decodes");
    let id = dereth_primitives::num::hash::str_hash(characterinfo::string::INNATES.as_bytes());
    let row = t
        .strings
        .into_iter()
        .find(|(k, _)| *k == id)
        .unwrap_or_else(|| panic!("{} is not in the table", characterinfo::string::INNATES))
        .1;
    let joined = row.strings.join("");
    assert!(
        joined.contains("\\n"),
        "{} holds no literal backslash-n, so this file is pointed at nothing: {joined:?}",
        characterinfo::string::INNATES
    );
    assert!(
        !joined.contains('\n'),
        "the dat already holds a real newline; the escape is not what separates these lines"
    );
}

// =============================================================================================
// 2. The defect — what the pane composes
// =============================================================================================

/// Behaviour: ui.text.an-escaped-line-break-in-a-string-table-reaches-the-pane-as-a-break
/// The six sections reach the pane as **text with real line breaks**, the way the client draws
/// them, and no backslash survives into the element.
#[test]
fn the_character_sheet_reaches_the_pane_unescaped() {
    let view = Burdened;
    let (mut ui, mut s, mut panels) = screen(&view);
    let _ = open_character_info(&mut ui, &mut s, &mut panels, &view);
    let text = panels.character_info.text.clone();
    let lines = text.lines().count();
    assert!(
        !text.contains('\\'),
        "a backslash reached the character sheet -- the unescape pass did not run:\n{text}"
    );
    assert!(
        lines > 6,
        "the sheet is {lines} lines before any wrapping; the six sections' own breaks never \
         became breaks:\n{text}"
    );
}

/// **What the player sees.** No glyph on the sheet is a backslash — with the escapes left in, the
/// pane literally draws one before every section.
#[test]
fn the_pane_draws_no_backslash() {
    let view = Burdened;
    let (mut ui, mut s, mut panels) = screen(&view);
    let pane = open_character_info(&mut ui, &mut s, &mut panels, &view);
    let g = composed(&mut ui, pane);
    assert!(!g.is_empty(), "the pane composed nothing at all");
    let slashes = g.iter().filter(|p| p.ch == u16::from(b'\\')).count();
    assert_eq!(
        slashes, 0,
        "the pane draws {slashes} literal backslashes on the screen"
    );
}

/// The unescaped sheet takes more lines than the run on paragraph and overflows the pane.
#[test]
fn the_unescaped_sheet_takes_more_lines_than_the_run_on_paragraph_and_overflows_the_pane() {
    let view = Burdened;
    let (mut ui, mut s, mut panels) = screen(&view);
    let pane = open_character_info(&mut ui, &mut s, &mut panels, &view);
    let box_ = ui.screen_box(pane);

    let unescaped = composed(&mut ui, pane);
    let unescaped_lines = line_count(&unescaped);
    let unescaped_extent = extent(&unescaped);
    let unescaped_low = unescaped.iter().map(|p| p.y).max().expect("glyphs");
    let unescaped_wrap: BTreeSet<i32> = unescaped.iter().map(|p| p.y).collect();

    // The same sheet with every break put back the way the dat stores it — the text this crate
    // composed before the pass existed.
    let escaped = panels.character_info.text.replace('\n', "\\n");
    ui.text_element_mut(pane)
        .expect("a text element")
        .set_text(&escaped);
    let run_on = composed(&mut ui, pane);
    let run_on_lines = line_count(&run_on);
    let run_on_extent = extent(&run_on);
    let run_on_wrap: BTreeSet<i32> = run_on.iter().map(|p| p.y).collect();

    assert_ne!(
        unescaped_wrap, run_on_wrap,
        "the two forms wrap to the same lines, so the escape makes no difference to the layout \
         and nothing here is load-bearing"
    );
    assert!(
        unescaped_lines > run_on_lines,
        "the unescaped sheet is {unescaped_lines} lines and the run-on form {run_on_lines}"
    );
    assert!(
        unescaped_extent > run_on_extent,
        "the unescaped sheet is {unescaped_extent} px and the run-on paragraph {run_on_extent} px, \
         so the escape is not what decides the height"
    );
    assert!(
        unescaped_extent > box_.height(),
        "the unescaped sheet is {unescaped_extent} px and fits the {} px pane",
        box_.height()
    );
    assert!(
        unescaped_low >= box_.y1,
        "the unescaped sheet's last line is at y {unescaped_low}, inside the pane {box_:?}"
    );
}

// =============================================================================================
// 3. How much of the shipped text this reaches — a measurement, not an assertion
// =============================================================================================
