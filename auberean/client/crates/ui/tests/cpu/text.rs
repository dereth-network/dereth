//! Text elements and tooltips: typing and editing at the caret, the caret's blink, strings in the
//! element's numbered fonts and colors, the view scrolling, and a tooltip showing and going.

use auberean_assets::ui::AttributeValue as V;
use auberean_primitives::DataId;
use auberean_ui::text::Color;
use auberean_ui::types::button_id;
use auberean_ui::types::text::key;
use auberean_ui::{Token, Ui};

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use auberean_text::StringInfo;
use auberean_ui::types::text::text_msg;
use auberean_ui::{Assets, Image};

use crate::fixture::{element, image, records, solid, state, ui, Records, FONT, LAYOUT};

/// A text element `name` at `rect` with these attributes and the test font.
fn text(name: &str, rect: [i16; 4], attrs: Vec<(&str, V)>) -> auberean_assets::ui::Element {
    let mut a = vec![("font", V::DataFile(DataId(FONT)))];
    a.extend(attrs);
    element(
        name,
        "text",
        rect,
        0,
        vec![state("stateless", 0, a, vec![], vec![])],
    )
}

fn entry(u: &mut Ui) -> auberean_ui::Handle {
    let l = u.load_layout(LAYOUT).unwrap();
    u.create_root(l, Token::of("Entry")).unwrap()
}

fn typed(u: &Ui, h: auberean_ui::Handle) -> String {
    u.text(h).unwrap_or_default()
}

/// What the input method is composing shows at the caret and isn't the entry's text: what it writes
/// is typed as any text, and when it gives up the picture is as it was. An entry that doesn't have
/// the input method on shows none of it.
///
/// Behaviour: ui.text.the-input-methods-composition-shows-at-the-caret
#[test]
fn the_input_methods_composition_shows_at_the_caret() {
    use auberean_ui::draw::{self, Surface};
    let e = text("Entry", [0, 0, 99, 19], vec![("editable", V::Bool(true))]);
    let mut u = ui(vec![e], vec![]);
    let h = entry(&mut u);
    let picture = |u: &Ui| {
        let mut s = Surface::new(100, 20);
        draw::draw(u, &mut s);
        s
    };
    u.set_composition("ab");
    let unfocused = picture(&u);
    u.set_composition("");
    assert!(picture(&u) == unfocused, "no entry has it on");

    u.pointer_move(10, 10);
    u.pointer_press();
    u.pointer_release();
    assert!(u.ime().is_some(), "the entry has it on");
    assert!(u.type_text("x"));
    let before = picture(&u);
    u.set_composition("ab");
    let composing = picture(&u);
    assert!(composing != before, "drawn at the caret");
    assert_eq!(typed(&u, h), "x", "not the entry's text");
    u.set_composition("");
    assert!(picture(&u) == before, "gone when it gives up");
    u.set_composition("ab");
    u.set_composition("");
    assert!(u.type_text("ab"));
    assert_eq!(typed(&u, h), "xab", "what it writes is typed");
}

/// An editable entry pressed takes the keyboard; typing goes in at the caret, no further than its
/// greatest length; Left and Home move the caret, Backspace drops the character before it and
/// Delete the one after; Shift with an arrow selects, and typing replaces the selection; Ctrl+C
/// copies it and Ctrl+V pastes over it.
///
/// Behaviour: ui.text.typing-and-editing-keys-work-at-the-caret
#[test]
fn typing_and_editing_keys_work_at_the_caret() {
    let e = text(
        "Entry",
        [0, 0, 199, 19],
        vec![
            ("editable", V::Bool(true)),
            ("selectable", V::Bool(true)),
            ("one_line", V::Bool(true)),
            ("max_chars", V::Int(8)),
        ],
    );
    let mut u = ui(vec![e], vec![]);
    let h = entry(&mut u);
    u.pointer_move(10, 10);
    u.pointer_press();
    u.pointer_release();
    assert_eq!(u.focus(), Some(h), "a press gives the entry the keyboard");
    assert!(u.type_text("abc"));
    u.character_input(0, key::LEFT);
    u.type_text("X");
    assert_eq!(typed(&u, h), "abXc", "typing goes in at the caret");
    u.backspace();
    assert_eq!(
        typed(&u, h),
        "abc",
        "Backspace drops the character before the caret"
    );
    u.character_input(0, key::HOME);
    u.character_input(0, key::DELETE);
    assert_eq!(typed(&u, h), "bc", "Delete drops the character after it");
    u.type_text("123456789");
    assert_eq!(
        typed(&u, h),
        "123456bc",
        "no further than its greatest length"
    );
    u.character_input(0, key::END);
    u.set_modifiers(true, false);
    u.character_input(0, key::LEFT);
    u.character_input(0, key::LEFT);
    u.set_modifiers(false, false);
    assert_eq!(u.selected_text(h), "bc");
    u.character_input(u16::from(b'z'), 0);
    assert_eq!(typed(&u, h), "123456z", "typing replaces the selection");
    u.select_all(h);
    u.set_modifiers(false, true);
    u.character_input(3, 0);
    assert_eq!(u.take_copied().as_deref(), Some("123456z"), "Ctrl+C copies");
    u.set_clipboard(Some("pq".into()));
    u.character_input(0x16, 0);
    u.set_modifiers(false, false);
    assert_eq!(typed(&u, h), "pq", "Ctrl+V pastes over the selection");
}

/// An editable entry's caret shows steadily for a second and a half after it moves, then blinks,
/// a second on and a second off.
///
/// Behaviour: ui.text.the-caret-shows-steadily-then-blinks
#[test]
fn the_caret_shows_steadily_then_blinks() {
    let e = text("Entry", [0, 0, 199, 19], vec![("editable", V::Bool(true))]);
    let mut u = ui(vec![e], vec![]);
    let h = entry(&mut u);
    u.use_time(0.0);
    u.pointer_move(10, 10);
    u.pointer_press();
    u.pointer_release();
    u.type_text("a");
    for (t, shown) in [
        (1.0, true),
        (1.6, false),
        (2.0, false),
        (2.7, true),
        (3.8, false),
    ] {
        u.use_time(t);
        assert_eq!(u.caret_shown(h), shown, "at {t} s");
    }
}

/// A string added in one of the element's numbered colors takes that color's red, green and blue
/// attributes (`font_color<n>_red` ...), one in the element's own color its `font_color_red` ...;
/// a channel the element lacks reads as full.
///
/// Behaviour: ui.text.strings-take-the-elements-numbered-font-colors
#[test]
fn strings_take_the_elements_numbered_font_colors() {
    let e = text(
        "Entry",
        [0, 0, 199, 19],
        vec![
            ("font_color_red", V::Float(1.0)),
            ("font_color_green", V::Float(0.0)),
            ("font_color3_red", V::Float(0.5)),
            ("font_color3_green", V::Float(0.25)),
            ("font_color3_blue", V::Float(0.75)),
        ],
    );
    let mut u = ui(vec![e], vec![]);
    let h = entry(&mut u);
    u.set_text(h, "");
    u.add_string_with_font(h, "x", 0, 3);
    u.add_string(h, "y");
    let g = &u.text_state(h).unwrap().glyphs.glyphs;
    assert_eq!(typed(&u, h), "xy");
    assert_eq!(
        g[0].color,
        Color {
            r: 0.5,
            g: 0.25,
            b: 0.75,
            a: 1.0
        }
    );
    assert_eq!(
        g[1].color,
        Color {
            r: 1.0,
            g: 0.0,
            b: 1.0,
            a: 1.0
        },
        "the missing blue reads as full"
    );
    assert_eq!(g[0].width, 6, "each glyph its width in the font");
}

/// A text taller than its element scrolls: to its end (the last lines at the foot), a page down
/// at a time within the text, and back home.
///
/// Behaviour: ui.text.the-view-scrolls-by-page-and-to-either-end
#[test]
fn the_view_scrolls_by_page_and_to_either_end() {
    let e = text("Entry", [0, 0, 99, 19], vec![]);
    let mut u = ui(vec![e], vec![]);
    let h = entry(&mut u);
    u.set_text(h, "a\nb\nc\nd\ne");
    u.recalculate_glyph_list(h);
    assert_eq!(u.total_lines(h), 5);
    u.scroll_to_end(h);
    assert_eq!(
        u.text_state(h).unwrap().view_y,
        30,
        "the last two lines show"
    );
    u.scroll_to_home(h);
    assert_eq!(u.text_state(h).unwrap().view_y, 0);
    u.vertical_page_increment(h);
    u.vertical_page_increment(h);
    assert_eq!(
        u.text_state(h).unwrap().view_y,
        30,
        "a page at a time, never past the end"
    );
}

/// A tooltip's owner and template, the pointer resting on the owner since time 0.
fn resting_on_a_tooltips_owner() -> Ui {
    let owner = element(
        "Owner",
        "field",
        [100, 100, 199, 199],
        5,
        vec![state(
            "stateless",
            0,
            vec![
                ("tooltips_on", V::Bool(true)),
                ("tooltip_ID", V::Id(Token::of("Tip").0)),
            ],
            vec![],
            vec![],
        )],
    );
    let tip = element(
        "Tip",
        "field",
        [0, 0, 57, 27],
        0,
        vec![state(
            "stateless",
            0,
            vec![("tooltip_text_ID", V::Id(Token::of("TipText").0))],
            vec![],
            vec![text("TipText", [4, 4, 53, 23], vec![])],
        )],
    );
    let mut u = ui(vec![owner, tip], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let o = u.create_root(l, Token::of("Owner")).unwrap();
    u.set_tooltip(o, "Hello");
    u.use_time(0.0);
    u.pointer_move(150, 150);
    u
}

/// The pointer passes through a tooltip: resting under it, it still finds what lies beneath.
///
/// Behaviour: ui.pointer.the-pointer-passes-through-a-tooltip
#[test]
fn the_pointer_passes_through_a_tooltip() {
    let mut u = resting_on_a_tooltips_owner();
    let owner = u.hit(150, 150).expect("the owner");
    u.use_time(0.3);
    let tip = u.tooltip().expect("the tooltip shows");
    let at = u.screen_rect(tip).expect("where it stands");
    assert!(at.contains(150, 150), "over the pointer: {at:?}");
    assert_eq!(u.hit(150, 150), Some(owner), "under it the owner");
}

/// The left button held back from anywhere holds a tooltip back: pressed where no element is (over
/// the world) and dragged onto the owner, no tooltip shows while it is down.
///
/// Behaviour: ui.tooltip.the-left-button-down-anywhere-holds-a-tooltip-back
#[test]
fn the_left_button_down_over_the_world_holds_a_tooltip_back() {
    let mut u = resting_on_a_tooltips_owner();
    u.pointer_move(10, 10);
    u.pointer_button(button_id::LEFT, true);
    u.pointer_move(150, 150);
    u.use_time(5.0);
    u.use_time(6.0);
    assert!(u.tooltip().is_none(), "held back while the button is down");
}

/// A tooltip is drawn in front of everything, also of the first on the screen at its own depth.
///
/// Behaviour: ui.draw.of-equal-depths-the-first-on-the-screen-is-drawn-in-front
#[test]
fn a_tooltip_stays_in_front_of_the_first_at_its_depth() {
    let owner = element(
        "Owner",
        "field",
        [0, 0, 99, 99],
        10,
        vec![state(
            "stateless",
            0,
            vec![
                ("tooltips_on", V::Bool(true)),
                ("tooltip_ID", V::Id(Token::of("Tip").0)),
            ],
            vec![image(1, 0)],
            vec![],
        )],
    );
    let tip = element(
        "Tip",
        "field",
        [0, 0, 57, 27],
        10,
        vec![state(
            "stateless",
            0,
            vec![("tooltip_text_ID", V::Id(Token::of("TipText").0))],
            vec![image(2, 0)],
            vec![text("TipText", [4, 4, 53, 23], vec![])],
        )],
    );
    let mut u = ui(
        vec![owner, tip],
        vec![
            (1, solid(100, 100, [0, 0, 255, 255])),
            (2, solid(58, 28, [0, 255, 0, 255])),
        ],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    let o = u.create_root(l, Token::of("Owner")).unwrap();
    u.set_tooltip(o, "Hello");
    u.use_time(0.0);
    u.pointer_move(10, 10);
    u.use_time(0.3);
    let shown = u.tooltip().expect("the tooltip");
    let at = u.screen_rect(shown).unwrap();
    let mut screen = auberean_ui::draw::Surface::new(1024, 768);
    auberean_ui::draw::draw(&u, &mut screen);
    assert_eq!(screen.get(at.x0 + 1, at.y0 + 1), [0, 255, 0, 255]);
}

/// The player's tooltip delay is how long the pointer rests: lengthened, nothing shows after the
/// quarter second; shortened to no more than the rest so far, it shows at once.
///
/// Behaviour: ui.tooltip.the-players-delay-and-switch-rule-tooltips
#[test]
fn the_players_tooltip_delay_is_how_long_the_pointer_rests() {
    let mut u = resting_on_a_tooltips_owner();
    u.set_tooltip_preferences(Some(2.0), None);
    u.use_time(1.0);
    assert!(u.tooltip().is_none(), "two seconds now, not a quarter");
    u.set_tooltip_preferences(Some(1.5), None);
    assert!(u.tooltip().is_none(), "rested one second of one and a half");
    u.set_tooltip_preferences(Some(1.0), None);
    assert!(u.tooltip().is_some(), "rested exactly as long as asked");
}

/// Switched off, no tooltip shows however long the pointer rests, and the one up goes; switched
/// back on, the element under the pointer shows its own at once.
///
/// Behaviour: ui.tooltip.the-players-delay-and-switch-rule-tooltips
#[test]
fn the_players_tooltip_switch_turns_them_off_and_back_on() {
    let mut u = resting_on_a_tooltips_owner();
    u.use_time(0.3);
    assert!(u.tooltip().is_some());
    u.set_tooltip_preferences(None, Some(false));
    assert!(u.tooltip().is_none(), "the one up goes");
    u.use_time(5.0);
    assert!(u.tooltip().is_none(), "none shows, however long");
    u.set_tooltip_preferences(None, Some(true));
    assert!(u.tooltip().is_some(), "back on, at once");
}

/// Once the pointer has rested a quarter of a second on an element that shows tooltips, its
/// tooltip template comes up in front with the element's tip text, below and right of the pointer;
/// it goes when the pointer moves to another element.
///
/// Behaviour: ui.tooltip.a-tooltip-shows-once-the-pointer-rests-and-goes-when-it-leaves
#[test]
fn a_tooltip_shows_once_the_pointer_rests_and_goes_when_it_leaves() {
    let owner = element(
        "Owner",
        "field",
        [100, 100, 199, 199],
        5,
        vec![state(
            "stateless",
            0,
            vec![
                ("tooltips_on", V::Bool(true)),
                ("tooltip_ID", V::Id(Token::of("Tip").0)),
            ],
            vec![],
            vec![],
        )],
    );
    let tip_text = text("TipText", [4, 4, 53, 23], vec![]);
    let tip = element(
        "Tip",
        "field",
        [0, 0, 57, 27],
        0,
        vec![state(
            "stateless",
            0,
            vec![("tooltip_text_ID", V::Id(Token::of("TipText").0))],
            vec![],
            vec![tip_text],
        )],
    );
    let other = element("Other", "field", [300, 300, 399, 399], 5, vec![]);
    let mut u = ui(vec![owner, tip, other], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let o = u.create_root(l, Token::of("Owner")).unwrap();
    u.create_root(l, Token::of("Other")).unwrap();
    u.set_tooltip(o, "Hello");
    u.use_time(0.0);
    u.pointer_move(150, 150);
    u.use_time(0.1);
    assert!(u.tooltip().is_none(), "not before the delay");
    u.use_time(0.3);
    let t = u.tooltip().expect("the tooltip shows");
    assert_eq!(u.roots()[0], t, "in front of everything");
    let field = u.child_recursive(t, Token::of("TipText")).unwrap();
    assert_eq!(u.text(field).as_deref(), Some("Hello"));
    let r = u.screen_rect(t).unwrap();
    assert_eq!(
        (r.x0, r.y0),
        (150, 150),
        "at the pointer (no cursor to step past)"
    );
    assert_eq!(
        (r.width(), r.height()),
        (30 + 8, 10 + 8),
        "sized so its text element holds the text"
    );
    u.pointer_move(350, 350);
    assert!(u.tooltip().is_none(), "gone once the pointer leaves");

    u.pointer_move(150, 150);
    u.use_time(0.7);
    assert!(u.tooltip().is_some(), "shown again");
    u.remove(o);
    assert!(
        u.tooltip().is_none(),
        "gone with its element, the pointer unmoved"
    );
}

/// A window hidden takes the tooltip of an element in it, the pointer unmoved.
///
/// Behaviour: ui.tooltip.a-tooltip-goes-with-the-window-its-owner-lies-in
#[test]
fn a_tooltip_goes_with_the_window_its_owner_lies_in() {
    let owner = element(
        "Owner",
        "field",
        [0, 0, 99, 99],
        5,
        vec![state(
            "stateless",
            0,
            vec![
                ("tooltips_on", V::Bool(true)),
                ("tooltip_ID", V::Id(Token::of("Tip").0)),
            ],
            vec![],
            vec![],
        )],
    );
    let window = element(
        "Window",
        "field",
        [100, 100, 199, 199],
        5,
        vec![state("stateless", 0, vec![], vec![], vec![owner])],
    );
    let tip = element(
        "Tip",
        "field",
        [0, 0, 57, 27],
        0,
        vec![state(
            "stateless",
            0,
            vec![("tooltip_text_ID", V::Id(Token::of("TipText").0))],
            vec![],
            vec![text("TipText", [4, 4, 53, 23], vec![])],
        )],
    );
    let mut u = ui(vec![window, tip], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let w = u.create_root(l, Token::of("Window")).unwrap();
    let o = u
        .child_recursive(w, Token::of("Owner"))
        .expect("in the window");
    u.set_tooltip(o, "Hello");
    u.use_time(0.0);
    u.pointer_move(150, 150);
    u.use_time(0.3);
    assert!(u.tooltip().is_some(), "shown while the pointer rests");
    u.set_visible(w, false);
    assert!(u.tooltip().is_none(), "gone with the window");
    u.use_time(5.0);
    assert!(u.tooltip().is_none(), "and stays gone");
}

/// The test's records with string downloads: a reference to [`LACKING`] waits until the test
/// says its tables came; an entry writes as `t<token>`, a literal as itself.
struct Downloads {
    records: Records,
    started: Mutex<u32>,
    waiting: Mutex<BTreeSet<u32>>,
    released: Mutex<Vec<u32>>,
}

/// The string table the data lacks.
const LACKING: u32 = 0x2500_0002;

impl Assets for Downloads {
    fn layout(&self, id: u32) -> Option<Arc<auberean_assets::ui::UiLayout>> {
        self.records.layout(id)
    }
    fn image(&self, id: u32) -> Option<Arc<Image>> {
        self.records.image(id)
    }
    fn font(&self, id: u32) -> Option<Arc<auberean_assets::ui::Font>> {
        self.records.font(id)
    }
    fn string(&self, _table: u32, _token: u32) -> Option<String> {
        None
    }
    fn element_text(&self, info: &StringInfo) -> Vec<u16> {
        let s = match &info.literal {
            Some(l) => String::from_utf16_lossy(l),
            None => format!("t{}", info.string_id),
        };
        s.encode_utf16().collect()
    }
    fn start_string_download(&self, info: &StringInfo) -> Option<u32> {
        if info.table != LACKING {
            return None;
        }
        let mut n = self.started.lock().unwrap();
        *n += 1;
        self.waiting.lock().unwrap().insert(*n);
        Some(*n)
    }
    fn string_download_waiting(&self, n: u32) -> bool {
        self.waiting.lock().unwrap().contains(&n)
    }
    fn release_string_download(&self, n: u32) {
        self.released.lock().unwrap().push(n);
    }
}

fn downloading(e: auberean_assets::ui::Element) -> (Ui, Arc<Downloads>) {
    let d = Arc::new(Downloads {
        records: records(vec![e], vec![]),
        started: Mutex::new(0),
        waiting: Mutex::new(BTreeSet::new()),
        released: Mutex::new(Vec::new()),
    });
    (Ui::new(d.clone(), 1024, 768), d)
}

fn said_done(u: &mut Ui, h: auberean_ui::Handle) -> bool {
    u.take_messages()
        .iter()
        .any(|m| m.from == h && m.what == text_msg::DOWNLOAD_DONE)
}

/// A string reference given to a text element whose tables the data lacks is added only once
/// they have come, at the element's next tick, and the element says its download is done; what
/// is added after it waits behind it, in order. A reference whose tables are at hand is added at
/// once.
///
/// Behaviour: ui.text.a-reference-waits-for-its-string-tables
#[test]
fn a_reference_waits_for_its_string_tables_and_what_follows_waits_behind_it() {
    let (mut u, d) = downloading(text("Entry", [0, 0, 199, 19], vec![]));
    let h = entry(&mut u);
    assert!(u.add_string_info(h, &StringInfo::entry(0x2500_0001, 1)));
    assert_eq!(typed(&u, h), "t1", "its tables at hand: added at once");
    assert!(!u.add_string_info(h, &StringInfo::entry(LACKING, 2)));
    assert!(u.add_string(h, " after"));
    u.use_time(1.0);
    assert_eq!(typed(&u, h), "t1", "both wait for the tables");
    u.take_messages();
    d.waiting.lock().unwrap().clear();
    u.use_time(2.0);
    assert_eq!(typed(&u, h), "t1t2 after", "added in order once they came");
    assert!(said_done(&mut u, h));
}

/// An element's entry of its string table goes in the way a string reference does, written by the
/// client, not as the table stores it.
///
/// Behaviour: ui.text.an-entry-of-the-string-table-is-a-reference
#[test]
fn an_entry_of_the_string_table_is_a_reference() {
    let (mut u, _) = downloading(text("Entry", [0, 0, 199, 19], vec![]));
    let h = entry(&mut u);
    u.set_string(h, 7);
    assert_eq!(typed(&u, h), "t7");
}

/// A text element's waiting strings are dropped when its text is replaced, and when it goes:
/// each lets go of the download it waits on.
///
/// Behaviour: ui.text.dropped-references-let-their-downloads-go
#[test]
fn a_text_element_lets_go_of_its_downloads_when_its_text_is_replaced_or_it_goes() {
    let (mut u, d) = downloading(text("Entry", [0, 0, 199, 19], vec![]));
    let h = entry(&mut u);
    u.add_string_info(h, &StringInfo::entry(LACKING, 2));
    u.set_string_info(h, &StringInfo::entry(LACKING, 3));
    assert_eq!(*d.released.lock().unwrap(), vec![1]);
    u.remove(h);
    assert_eq!(*d.released.lock().unwrap(), vec![1, 2]);
}

/// An element's own tip given as a string reference goes to the tooltip's text element as one:
/// while the string table it reads is on its way the tooltip is up with no text, and its text
/// shows once the table has come.
///
/// Behaviour: ui.tooltip.a-tip-given-as-a-reference-waits-for-its-string-tables
#[test]
fn a_tip_given_as_a_reference_waits_for_its_string_tables() {
    let owner = element(
        "Owner",
        "field",
        [100, 100, 199, 199],
        5,
        vec![state(
            "stateless",
            0,
            vec![
                ("tooltips_on", V::Bool(true)),
                ("tooltip_ID", V::Id(Token::of("Tip").0)),
            ],
            vec![],
            vec![],
        )],
    );
    let tip = element(
        "Tip",
        "field",
        [0, 0, 57, 27],
        0,
        vec![state(
            "stateless",
            0,
            vec![("tooltip_text_ID", V::Id(Token::of("TipText").0))],
            vec![],
            vec![text("TipText", [4, 4, 53, 23], vec![])],
        )],
    );
    let d = Arc::new(Downloads {
        records: records(vec![owner, tip], vec![]),
        started: Mutex::new(0),
        waiting: Mutex::new(BTreeSet::new()),
        released: Mutex::new(Vec::new()),
    });
    let mut u = Ui::new(d.clone(), 1024, 768);
    let l = u.load_layout(LAYOUT).unwrap();
    let o = u.create_root(l, Token::of("Owner")).unwrap();
    u.set_tooltip_info(o, &StringInfo::entry(LACKING, 4));
    u.use_time(0.0);
    u.pointer_move(150, 150);
    u.use_time(0.3);
    let t = u.tooltip().expect("the tooltip shows");
    let field = u.child_recursive(t, Token::of("TipText")).unwrap();
    assert_eq!(u.text(field).as_deref(), Some(""), "waiting on the table");
    d.waiting.lock().unwrap().clear();
    u.use_time(0.35);
    assert_eq!(u.text(field).as_deref(), Some("t4"));
}

/// A font of these characters, each `width` pixels wide and 10 high, with blank glyphs and the
/// map of its characters, as a font read from the files has.
fn font_of(chars: impl IntoIterator<Item = u16>, width: u32) -> Arc<auberean_assets::ui::Font> {
    let glyphs: Vec<auberean_assets::ui::Glyph> = chars
        .into_iter()
        .enumerate()
        .map(|(i, c)| auberean_assets::ui::Glyph {
            width,
            code_point: c,
            unused: 0,
            offset: u32::try_from(i).unwrap_or(0) * width,
        })
        .collect();
    let total_width = u32::try_from(glyphs.len()).unwrap_or(0) * width;
    Arc::new(auberean_assets::ui::Font {
        max_height: 10,
        max_width: width,
        average_width: width,
        total_width,
        antialiased: true,
        pixels: vec![0; (total_width * 10) as usize],
        character_map: auberean_assets::ui::CharacterMap::of(&glyphs),
        glyphs,
    })
}

/// A string with a character the element's font lacks switches the element to the first fallback
/// font that holds all of it (the Unicode font, then the second); when neither holds it, or the
/// element has no font, the font stays. The fallback lasts for that string: the next one, a typed
/// character or a measured string starts from the element's own font.
///
/// Behaviour: ui.text.a-character-the-font-lacks-switches-to-a-fallback-font
#[test]
fn a_character_the_font_lacks_switches_to_a_fallback_font() {
    const UNICODE: u32 = 0x4200_0010;
    const SECOND: u32 = 0x4200_0011;
    let names = ["A", "B", "C", "D", "E"];
    let mut elements: Vec<_> = names
        .iter()
        .map(|n| text(n, [0, 0, 299, 19], vec![]))
        .collect();
    elements.push(element(
        "Bare",
        "text",
        [0, 0, 299, 19],
        0,
        vec![state("stateless", 0, vec![], vec![], vec![])],
    ));
    let mut r = records(elements, vec![]);
    r.fonts
        .insert(UNICODE, font_of((0x20u16..0x7F).chain([0xE9]), 7));
    r.fonts
        .insert(SECOND, font_of((0x20u16..0x7F).chain([0x416]), 9));
    r.enums.insert((0x4000_0004, 0x4000_000B), UNICODE);
    r.enums.insert((0x4000_0005, 0x4000_000B), SECOND);
    let mut u = Ui::new(Arc::new(r), 1024, 768);
    let l = u.load_layout(LAYOUT).unwrap();
    let make = |u: &mut Ui, n: &str| u.create_root(l, Token::of(n)).unwrap();
    let font = |u: &Ui, h| u.text_state(h).unwrap().font;
    let widths = |u: &Ui, h| -> Vec<i32> {
        u.text_state(h)
            .unwrap()
            .glyphs
            .glyphs
            .iter()
            .map(|g| g.width)
            .collect()
    };

    let a = make(&mut u, "A");
    u.add_string(a, "ab");
    assert_eq!(font(&u, a), FONT, "a string the font holds keeps it");
    u.add_string(a, "café");
    assert_eq!(font(&u, a), FONT, "the font is back after the string");
    assert_eq!(
        widths(&u, a),
        [6, 6, 7, 7, 7, 7],
        "the string in the fallback font"
    );
    u.add_string(a, "x");
    assert_eq!(
        widths(&u, a),
        [6, 6, 7, 7, 7, 7, 6],
        "the next string in the element's own font"
    );

    let b = make(&mut u, "B");
    u.add_string(b, "\u{416}");
    assert_eq!(
        widths(&u, b),
        [9],
        "the first fallback lacks it: the second"
    );

    let c = make(&mut u, "C");
    u.add_string(c, "\u{2603}");
    assert_eq!(font(&u, c), FONT, "neither holds it");

    let d = make(&mut u, "D");
    u.add_string(d, "line\none");
    assert_eq!(font(&u, d), FONT, "a new line needs no glyph");

    let e = make(&mut u, "E");
    let measured = u.inq_size_with_margins(e, "é");
    assert_eq!(font(&u, e), FONT, "measuring leaves the font as it was");
    assert_eq!(
        measured.map(|m| m.0),
        Some(7),
        "measured in the fallback font"
    );

    let bare = make(&mut u, "Bare");
    u.add_string(bare, "é");
    assert_eq!(font(&u, bare), 0, "an element without a font keeps none");
}

/// With the mapping naming a default font, an element with no font of its own starts in it; one
/// with a font of its own has that.
///
/// Behaviour: ui.text.an-element-with-no-font-starts-in-the-default-font
#[test]
fn an_element_with_no_font_starts_in_the_default_font() {
    const DEFAULT: u32 = 0x4200_0012;
    let bare = element(
        "Bare",
        "text",
        [0, 0, 299, 19],
        0,
        vec![state("stateless", 0, vec![], vec![], vec![])],
    );
    let mut r = records(vec![bare, text("Own", [0, 0, 299, 19], vec![])], vec![]);
    r.fonts.insert(DEFAULT, font_of(0x20u16..0x7F, 9));
    r.enums.insert((0x4000_003E, 0x4000_000B), DEFAULT);
    let mut u = Ui::new(Arc::new(r), 1024, 768);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Bare")).unwrap();
    assert_eq!(u.text_state(h).unwrap().font, DEFAULT);
    u.add_string(h, "ab");
    assert_eq!(u.text_state(h).unwrap().glyphs.glyphs[0].width, 9);
    let own = u.create_root(l, Token::of("Own")).unwrap();
    assert_eq!(u.text_state(own).unwrap().font, FONT);
}

/// A one-line entry keeps the caret in view: the view stays while the caret is within the width
/// past it, then stands one width before the caret; going back past its start puts the view at
/// the caret.
///
/// Behaviour: ui.text.a-one-line-entry-keeps-the-caret-in-view
#[test]
fn a_one_line_entry_keeps_the_caret_in_view() {
    let e = text(
        "Entry",
        [0, 0, 19, 19],
        vec![("editable", V::Bool(true)), ("one_line", V::Bool(true))],
    );
    let mut u = ui(vec![e], vec![]);
    let h = entry(&mut u);
    let view = |u: &Ui| u.text_state(h).unwrap().view_x;
    u.pointer_move(10, 10);
    u.pointer_press();
    u.pointer_release();
    let advance = {
        u.type_text("a");
        let w = u.text_state(h).unwrap().glyphs.glyphs[0].width;
        u.backspace();
        w
    };
    let width = 20;
    let fit = width / advance;
    for _ in 0..fit {
        u.type_text("a");
        assert_eq!(view(&u), 0, "within the width the view stays");
    }
    for _ in 0..4 {
        u.type_text("a");
    }
    let at_end = (fit + 4) * advance - width;
    assert_eq!(view(&u), at_end, "one width before the caret");
    u.character_input(0, key::HOME);
    assert_eq!(view(&u), 0, "back at the start");
    u.character_input(0, key::END);
    assert_eq!(view(&u), at_end, "at the end again");
}

/// An element shown with `activate_on_show` takes the keyboard, unless the use key is held: it may
/// be what showed the element, and letting it go would press what took the keyboard.
///
/// Behaviour: ui.element.the-use-key-held-keeps-a-shown-element-off-the-keyboard
#[test]
fn the_use_key_held_keeps_a_shown_element_off_the_keyboard() {
    use auberean_ui::types::key_id;
    let mut u = ui(
        vec![text(
            "Entry",
            [10, 20, 109, 39],
            vec![
                ("editable", V::Bool(true)),
                ("activate_on_show", V::Bool(true)),
            ],
        )],
        vec![],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    let entry = u.create_root(l, Token::of("Entry")).unwrap();
    u.set_visible(entry, false);
    u.deactivate();
    u.key(key_id::USE, true);
    u.set_visible(entry, true);
    assert_eq!(u.active(), None, "held, it stays off the keyboard");
    u.key(key_id::USE, false);
    u.set_visible(entry, false);
    u.set_visible(entry, true);
    assert_eq!(u.active(), Some(entry), "let go, it takes the keyboard");
}

/// An editable entry that takes the keyboard turns the host's input method on over the part of it
/// that shows, in its font and color, and giving the keyboard up turns it off; an entry marked
/// `no_IME` never turns it on.
///
/// Behaviour: ui.text.an-entry-taking-the-keyboard-turns-the-input-method-on
#[test]
fn an_entry_taking_the_keyboard_turns_the_input_method_on() {
    let editable = |name: &str, extra: Vec<(&str, V)>| {
        let mut a = vec![("editable", V::Bool(true))];
        a.extend(extra);
        text(name, [10, 20, 109, 39], a)
    };
    let mut u = ui(
        vec![
            editable("Entry", vec![]),
            editable("Plain", vec![("no_IME", V::Bool(true))]),
        ],
        vec![],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    let entry = u.create_root(l, Token::of("Entry")).unwrap();
    let plain = u.create_root(l, Token::of("Plain")).unwrap();
    assert!(u.ime().is_none());
    assert!(u.activate(entry));
    let ime = *u.ime().expect("on");
    assert_eq!(ime.area, auberean_ui::Rect::new(10, 20, 109, 39));
    assert_eq!(ime.font, FONT);
    assert_eq!(ime.color, Color::WHITE);
    u.deactivate();
    assert!(u.ime().is_none(), "the keyboard given up turns it off");
    assert!(u.activate(plain));
    assert!(u.ime().is_none(), "no_IME keeps it off");
}
