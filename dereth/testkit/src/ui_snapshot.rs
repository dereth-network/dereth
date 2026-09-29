//! [`UiSnapshot`] -- a whole panel's shape, as one owned value.
//!
//! For the many claims that are about what the shipped element tree looks like after something
//! happened. Each of them needs the same four helpers -- walk to the gameplay screen, find an
//! element by id, read `region.flags.visible`, collect the text off a list box's children -- and
//! then asserts three or four fields of it.
//!
//! This is those four, once, plus the frame the client drew:
//!
//! | reader | what it answers |
//! |---|---|
//! | [`UiSnapshot::assert_visible`] | that an element is in the tree **and** its region is visible |
//! | [`UiSnapshot::text_of`] | the glyphs that element composed, as text |
//! | [`UiSnapshot::lines_of`] | the same for each text child, which is what a list box's rows are |
//! | [`UiSnapshot::screen_box`] | the absolute rectangle it occupies |
//! | [`UiSnapshot::tree_text`] | the whole subtree, one element per line -- what a golden holds |
//!
//! and the things such a claim needs that the tree alone cannot answer -- glyph colour, state and
//! art:
//!
//! | reader | what it answers |
//! |---|---|
//! | [`UiSnapshot::runs_of`] | the `(font, colour)` of every glyph the element composed |
//! | [`UiSnapshot::run_spans_of`] | the same, folded into runs -- "white, then green" |
//! | [`UiSnapshot::font_colours_of`] | the colours the layout **declared** for it |
//! | [`UiSnapshot::state_of`] | which state it is in |
//! | [`UiSnapshot::art_of`] | the art that state declares |
//! | [`UiSnapshot::runs_text`] | the runs as a golden |
//!
//! A buffed skill drawn green and a debuffed one drawn red is a property of the *composed glyph*
//! and not of the element, so the first two are where that claim lives. The declared palette is
//! beside them because the two together are the claim: a scenario that only read the glyph colour
//! could not tell a right index from a wrong constant, and one that only read the array could not
//! tell whether the panel used it.
//!
//! # Owned, and why
//!
//! It is a snapshot and not a view: a scenario reads the tree *when the claim is about*, then runs
//! more frames, then books the claim in an `assert_behaviour` closure that can only see a
//! [`crate::ScenarioView`]. A borrowed handle cannot cross that, which is why a tree assertion
//! written without it ends with a tuple of extracted fields and a `move` closure. One owned value is the same
//! discipline with the extraction done once.
//!
//! # The golden
//!
//! [`UiSnapshot::assert_tree`] compares [`UiSnapshot::tree_text`] against
//! `tests/golden/ui/<name>.txt` through [`crate::golden`], so `DERETH_TEST_GOLDEN=1` rewrites it and the
//! diff is what a reader reads. It is for the stations whose claim really is *the whole panel
//! looks like this* -- a shape with a dozen rows in it that four `assert_eq!`s only partly pin.
//! A claim about one field stays an assertion on one field; a golden is not a way to avoid saying
//! what changed.
//!
//! **What a golden of a tree cannot witness**: anything the element tree does not carry. A colour
//! the renderer resolves, a texture the dat supplies, a pixel. Those are `dereth-render`'s and the
//! `gpu` tier's, and a line here saying an image id is not a line saying the image was drawn.

use std::collections::BTreeMap;

use dereth_client_contract::GameSnapshot;
use dereth_ui::desc::{MediaFields, StateDesc};
use dereth_ui::region::Box2D;
use dereth_ui::{ElemHandle, ElementId, UiDrawCmd, UiSystem};

/// The data ids of the art one state declares, in the state's own media order.
///
/// Only the four kinds of media that *are* a picture: an image, an alpha mask, the
/// frames of an animation, a cursor. A jump, a pause, a message, a sound or a fade is a step in the
/// state's script and not something the state draws, so none of them contributes an id -- and a
/// movie's media is a file name rather than a data id, so it does not either.
fn media_ids(state: &StateDesc) -> Vec<u32> {
    let mut out = Vec::new();
    for m in &state.media {
        match &m.fields {
            MediaFields::Image { file, .. }
            | MediaFields::Alpha { file }
            | MediaFields::Cursor { file, .. } => out.push(file.0),
            MediaFields::Anim { frames, .. } => out.extend(frames.iter().map(|f| f.0)),
            _ => {}
        }
    }
    out
}

/// The colour array attribute `0x1B` declares for an element, in index order.
///
/// The same read as `dereth_ui_screens::panels::statmgmt::font_color_at`, over the
/// whole array rather than one index, so that a snapshot carries the palette and a scenario does
/// not have to hold the live tree to ask for it.
fn declared_font_colours(node: &dereth_ui::element::ElementNode) -> Vec<u32> {
    use dereth_ui::PropertyValue;

    const ATTR_FONT_COLOR: u32 = dereth_ui_screens::panels::statmgmt::ATTR_FONT_COLOR;
    let merged = node.merged_properties();
    let Some(PropertyValue::Array(a)) = merged.get(ATTR_FONT_COLOR) else {
        return Vec::new();
    };
    a.iter()
        .map(|e| match &e.value {
            PropertyValue::Color(c) => *c,
            _ => 0,
        })
        .collect()
}

/// One element of the shipped tree, as a scenario reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementFacts {
    /// The shipped layout's own id.
    pub id: u32,
    /// How deep under the screen root it sits, so a tree can be printed as a tree.
    pub depth: usize,
    /// `region.flags.visible`.
    pub visible: bool,
    /// The absolute rectangle returned for this element in screen coordinates.
    pub screen: Box2D,
    /// The clipped rectangle. An element whose clip box is invalid is on screen and drawing
    /// nothing, which is a different thing from being invisible.
    pub clip: Box2D,
    /// The glyphs this element composed, as text. Empty for everything that is not a text
    /// element -- and also for a text element that composed none, which is a different thing:
    /// [`Self::is_text`] tells them apart, and an **empty text element is a real defect** (a
    /// notice bubble with no words in it is one of the things this crate exists to catch).
    pub text: String,
    /// Whether this element is a text element at all.
    pub is_text: bool,
    /// One `(font, colour)` per **composed glyph**, in the order [`Self::text`] reads.
    ///
    /// Font-aware text composition is how a panel writes two colours into one element, and
    /// several panel claims are about exactly that second colour -- a buffed skill drawn green, a
    /// debuffed one red. It is a property of the composed
    /// glyph and not of the element, so it lives here and not in the property table.
    ///
    /// Empty for everything that is not a text element. `font` is an index into the element's own
    /// font set, not a font id.
    pub runs: Vec<(u32, u32)>,
    /// Which of the element's declared states it is currently in.
    /// `dereth_ui::element::state::NORMAL`, `ROLLOVER`, `FOCUSED`, `ACTIVE` name the shipped ones.
    ///
    /// A claim about a *button* is about its state and its picture.
    pub state: u32,
    /// The art the element's **current state** declares, as the data ids of its image, alpha,
    /// animation-frame and cursor media, in the state's own order.
    ///
    /// It is what the state says it draws, not what the renderer put on a surface: this crate has
    /// no device, and a line here saying an image id is not a line saying the image was drawn.
    pub art: Vec<u32>,
    /// The colours attribute `0x1B` **declares** for this element, in index order -- which is a
    /// different thing from [`Self::runs`]: this is the palette the layout shipped, and `runs` is
    /// what the panel actually drew out of it.
    ///
    /// Selecting font-colour index `i` is how a panel says "draw this number in the raised colour"; a scenario
    /// that only asserted the glyph colour could not tell a right index from a wrong constant, and
    /// one that only asserted the array could not tell whether the panel used it. Captured for
    /// text elements only.
    pub font_colours: Vec<u32>,
}

impl ElementFacts {
    /// [`Self::runs`] collapsed to its runs: `(font, colour, how many glyphs)`, consecutive glyphs
    /// with the same font and colour folded into one.
    ///
    /// This is the shape a claim about two colours in one element is written in -- "the name is
    /// white and the number after it is green" is two entries -- and it is what
    /// [`UiSnapshot::runs_text`] prints.
    #[must_use]
    pub fn run_spans(&self) -> Vec<(u32, u32, usize)> {
        let mut out: Vec<(u32, u32, usize)> = Vec::new();
        for &(font, colour) in &self.runs {
            match out.last_mut() {
                Some(last) if last.0 == font && last.1 == colour => last.2 += 1,
                _ => out.push((font, colour, 1)),
            }
        }
        out
    }
}

/// A whole client's drawn state, as one owned value.
///
/// **It is keyed by node and not by id.** A list box builds its rows from one template, so every
/// row of it is a live element carrying the *same* shipped element id; a snapshot keyed by id
/// would have reported one row's text as every row's, which is exactly the shape of defect a
/// station about a list box is written to catch. [`UiSnapshot::facts`] resolves an id to the first
/// node carrying it, matching the client's own first-match element lookup.
#[derive(Debug, Clone)]
pub struct UiSnapshot {
    /// The game model through the client's own projection -- the same value
    /// [`crate::HeadlessClient::snapshot`] answers.
    pub game: GameSnapshot,
    /// This frame's 2D blit list, in the order the client emitted it.
    pub draw: Vec<UiDrawCmd>,
    /// Every live element under the current screen's roots, in tree order.
    nodes: Vec<ElementFacts>,
    /// Each node's children, as indices into [`Self::nodes`].
    children: Vec<Vec<usize>>,
    /// The screen's own roots.
    roots: Vec<usize>,
    /// The first node carrying each id.
    by_id: BTreeMap<u32, usize>,
}

impl UiSnapshot {
    /// Walk `roots` and record everything under them.
    pub(crate) fn capture(
        game: GameSnapshot,
        draw: Vec<UiDrawCmd>,
        ui: &mut UiSystem,
        roots: &[ElemHandle],
    ) -> Self {
        let mut out = Self {
            game,
            draw,
            nodes: Vec::new(),
            children: Vec::new(),
            roots: Vec::new(),
            by_id: BTreeMap::new(),
        };
        for r in roots {
            if let Some(i) = out.walk(ui, *r, 0) {
                out.roots.push(i);
            }
        }
        out
    }

    fn walk(&mut self, ui: &mut UiSystem, h: ElemHandle, depth: usize) -> Option<usize> {
        let id = ui.node(h)?.element_id().0;
        let (visible, screen, clip) = (
            ui.node(h)?.region.flags.visible,
            ui.screen_box(h),
            ui.screen_clip_box(h),
        );
        // The state and the state's own media list, off the same node, before the
        // text element is borrowed mutably.
        let node = ui.node(h)?;
        let state = node.state.0;
        let art = node.current_state_desc().map(media_ids).unwrap_or_default();
        let composed = ui.text_element_mut(h).map(|t| {
            (
                t.glyphs.inq_text(false),
                t.glyphs
                    .glyphs
                    .iter()
                    .map(|g| (g.font, g.color))
                    .collect::<Vec<_>>(),
            )
        });
        let is_text = composed.is_some();
        let (text, runs) = composed.unwrap_or_default();
        // The declared palette, for text elements only: merging three property collections per
        // element over a whole tree is the one expensive thing in this walk.
        let font_colours = if is_text {
            ui.node(h).map(declared_font_colours).unwrap_or_default()
        } else {
            Vec::new()
        };
        let me = self.nodes.len();
        self.nodes.push(ElementFacts {
            id,
            depth,
            visible,
            screen,
            clip,
            is_text,
            text,
            runs,
            state,
            art,
            font_colours,
        });
        self.children.push(Vec::new());
        self.by_id.entry(id).or_insert(me);
        for k in ui.children(h) {
            if let Some(c) = self.walk(ui, k, depth + 1) {
                self.children[me].push(c);
            }
        }
        Some(me)
    }

    /// Whether the tree carries `id` at all.
    #[must_use]
    pub fn has(&self, id: ElementId) -> bool {
        self.by_id.contains_key(&id.0)
    }

    /// Everything recorded about the first node carrying `id`.
    ///
    /// # Panics
    /// Panics when `id` is not in the shipped tree. An element a scenario names and the layout
    /// does not carry is a wrong id, and answering `None` would let the scenario read it as
    /// "absent, as expected".
    #[must_use]
    pub fn facts(&self, id: ElementId) -> &ElementFacts {
        let i = self.by_id.get(&id.0).copied().unwrap_or_else(|| {
            panic!(
                "{id:?} is not under this screen's roots; the tree carries {} live elements and \
                 none of them is that one",
                self.nodes.len()
            )
        });
        &self.nodes[i]
    }

    /// Every node carrying `id`, in tree order -- which is more than one whenever a list box built
    /// its rows from one template.
    #[must_use]
    pub fn all_of(&self, id: ElementId) -> Vec<&ElementFacts> {
        self.nodes.iter().filter(|f| f.id == id.0).collect()
    }

    /// Whether `id` is in the tree and its region is visible.
    #[must_use]
    pub fn is_visible(&self, id: ElementId) -> bool {
        self.by_id
            .get(&id.0)
            .is_some_and(|i| self.nodes[*i].visible)
    }

    /// Assert that `id` is in the tree and visible.
    ///
    /// # Panics
    /// Panics with which of the two failed, because they are different defects: an absent element
    /// is a wrong id or a layout that did not load, and an invisible one is a panel that was never
    /// raised.
    pub fn assert_visible(&self, id: ElementId) {
        let f = self.facts(id);
        assert!(
            f.visible,
            "{id:?} is in the shipped tree and its region is not visible"
        );
    }

    /// The glyphs `id` composed, as text.
    ///
    /// # Panics
    /// As [`Self::facts`].
    #[must_use]
    pub fn text_of(&self, id: ElementId) -> &str {
        &self.facts(id).text
    }

    /// The text of each of `id`'s **text** children, in the tree's own order, **empty ones
    /// included** -- which is what a list box's rows are, and what a station asserting "no empty
    /// row" has to be able to see.
    ///
    /// # Panics
    /// As [`Self::facts`].
    #[must_use]
    pub fn lines_of(&self, id: ElementId) -> Vec<&str> {
        let i = self.by_id.get(&id.0).copied().unwrap_or_else(|| {
            panic!("{id:?} is not under this screen's roots");
        });
        self.children[i]
            .iter()
            .map(|c| &self.nodes[*c])
            .filter(|f| f.is_text)
            .map(|f| f.text.as_str())
            .collect()
    }

    /// [`Self::lines_of`] without the empty ones -- the rows a reader would say are there.
    ///
    /// # Panics
    /// As [`Self::facts`].
    #[must_use]
    pub fn rows_of(&self, id: ElementId) -> Vec<&str> {
        self.lines_of(id)
            .into_iter()
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// The absolute rectangle `id` occupies.
    ///
    /// # Panics
    /// As [`Self::facts`].
    #[must_use]
    pub fn screen_box(&self, id: ElementId) -> Box2D {
        self.facts(id).screen
    }

    // -------------------------------------------------------------------------------------
    // Font, colour, state and art.
    // -------------------------------------------------------------------------------------

    /// One `(font, colour)` per composed glyph of `id`. See [`ElementFacts::runs`].
    ///
    /// # Panics
    /// As [`Self::facts`].
    #[must_use]
    pub fn runs_of(&self, id: ElementId) -> &[(u32, u32)] {
        &self.facts(id).runs
    }

    /// [`Self::runs_of`] folded into `(font, colour, glyphs)` spans -- the shape "the name is
    /// white and the number after it is green" is written in.
    ///
    /// # Panics
    /// As [`Self::facts`].
    #[must_use]
    pub fn run_spans_of(&self, id: ElementId) -> Vec<(u32, u32, usize)> {
        self.facts(id).run_spans()
    }

    /// Whether every glyph of `id` is one run: one font, one colour, and as many glyphs as `text`
    /// has UTF-16 code units.
    ///
    /// The **code units** and not the characters, because that is what a glyph is; a claim about
    /// text outside the basic plane would be a claim about the code page of whatever machine ran
    /// the scenario, which this crate does not make.
    ///
    /// # Panics
    /// As [`Self::facts`].
    #[must_use]
    pub fn is_one_run(&self, id: ElementId, text: &str, font: u32, colour: u32) -> bool {
        let f = self.facts(id);
        f.runs.len() == text.encode_utf16().count()
            && f.runs
                .iter()
                .all(|&(g_font, g_colour)| g_font == font && g_colour == colour)
    }

    /// Which state `id` is in -- `dereth_ui::element::state::NORMAL` and its siblings.
    ///
    /// # Panics
    /// As [`Self::facts`].
    #[must_use]
    pub fn state_of(&self, id: ElementId) -> u32 {
        self.facts(id).state
    }

    /// The data ids of the art `id`'s **current state** declares. See [`ElementFacts::art`].
    ///
    /// # Panics
    /// As [`Self::facts`].
    #[must_use]
    pub fn art_of(&self, id: ElementId) -> &[u32] {
        &self.facts(id).art
    }

    /// The colours `id` **declares**, in index order. See [`ElementFacts::font_colours`].
    ///
    /// # Panics
    /// As [`Self::facts`].
    #[must_use]
    pub fn font_colours_of(&self, id: ElementId) -> &[u32] {
        &self.facts(id).font_colours
    }

    /// The runs of `id`, one span per line: the font index, the colour, how many glyphs, and the
    /// glyphs themselves. **This is the golden a colour change is witnessed by.**
    ///
    /// It is a second golden rather than a column of [`Self::tree_text`] on purpose: every UI
    /// golden already committed would have had to be rewritten to carry it, and a golden rewritten
    /// wholesale cannot be reviewed -- a reader could not tell a real change from a change of
    /// format. A claim about colour asks for this one by name.
    ///
    /// # Panics
    /// As [`Self::facts`].
    #[must_use]
    pub fn runs_text(&self, id: ElementId) -> String {
        use std::fmt::Write as _;
        let f = self.facts(id);
        let mut out = String::new();
        let mut at = 0usize;
        let units: Vec<u16> = f.text.encode_utf16().collect();
        for (font, colour, n) in f.run_spans() {
            let end = (at + n).min(units.len());
            let s = String::from_utf16_lossy(units.get(at..end).unwrap_or_default());
            let _ = writeln!(
                out,
                "font {font} {colour:#010X} x{n} {:?}",
                s.replace('\n', "\\n")
            );
            at = end;
        }
        out
    }

    /// How many live elements are under the screen's roots. A tree that came up empty is a layout
    /// that did not load, and a scenario asserting over it would be asserting over nothing.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree is empty. See [`Self::len`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The subtree under `id`, one element per line, indented by depth: the id in the shipped
    /// layout's own hexadecimal, whether it is visible, its rectangle, and its text when it has
    /// any. This is what a golden holds.
    ///
    /// Passing `None` prints every root.
    ///
    /// # Panics
    /// As [`Self::facts`], when `id` is given.
    #[must_use]
    pub fn tree_text(&self, id: Option<ElementId>) -> String {
        let mut out = String::new();
        match id {
            Some(id) => {
                let i = self.by_id.get(&id.0).copied().unwrap_or_else(|| {
                    panic!("{id:?} is not under this screen's roots");
                });
                self.print(&mut out, i, self.nodes[i].depth);
            }
            None => {
                for r in &self.roots {
                    self.print(&mut out, *r, 0);
                }
            }
        }
        out
    }

    fn print(&self, out: &mut String, i: usize, base: usize) {
        use std::fmt::Write as _;
        let f = &self.nodes[i];
        let indent = "  ".repeat(f.depth.saturating_sub(base));
        let shown = if f.visible { "visible" } else { "hidden " };
        let _ = write!(
            out,
            "{indent}{:#010X} {shown} [{},{} {}x{}]",
            f.id,
            f.screen.x0,
            f.screen.y0,
            f.screen.width(),
            f.screen.height()
        );
        if f.is_text {
            // The text is the client's own, and a newline inside it would break the
            // one-element-per-line shape a golden diff is read in.
            let _ = write!(out, " {:?}", f.text.replace('\n', "\\n"));
        }
        out.push('\n');
        for c in &self.children[i] {
            self.print(out, *c, base);
        }
    }

    /// Compare [`Self::tree_text`] against `tests/golden/ui/<name>.txt`, or rewrite it under
    /// `DERETH_TEST_GOLDEN=1`. See [`crate::golden`].
    ///
    /// # Panics
    /// Panics on the first differing line, and when the golden is absent on a run that is not
    /// rewriting.
    pub fn assert_tree(&self, name: &str, id: Option<ElementId>) {
        crate::golden::check(&format!("ui/{name}"), &self.tree_text(id));
    }
}
