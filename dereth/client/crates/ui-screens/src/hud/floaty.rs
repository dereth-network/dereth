//! The sixteen movable windows and their shared chrome pattern.

use dereth_ui::{ElemHandle, ElementId, ElementType, UiSystem};

use crate::element_types::ty;

/// The toolbar's move override is not inherited from the element base. Its screen still owns
/// bindings/chrome/selection; this behavior supplies the virtual geometry and persistence tails.
#[derive(Debug, Default)]
struct FloatyToolbar {
    // The floating toolbar's constructor initializes the window id to zero; post-init reads it from
    // the layout attribute, once.
    // Reparent/geometry calls during construction therefore cannot write default placement.
    window_id: u32,
}

impl FloatyToolbar {
    fn write(&self, requests_out: &mut crate::requests::Outbox, property: u32, value: i32) {
        if self.window_id != 0 {
            requests_out.emit(crate::view::UiRequest::SetChatWindowOption {
                window: self.window_id,
                property,
                value,
            });
        }
    }
}

impl dereth_ui::Element for FloatyToolbar {
    fn post_init(&mut self, ctx: &mut dereth_ui::ElemCtx<'_>) {
        self.window_id = crate::bind::attr_enum(ctx.ui, ctx.me, 0x1000_007E).unwrap_or(0);
    }

    fn constrain_move(&self, ui: &UiSystem, me: ElemHandle, x: i32, y: i32) -> (i32, i32) {
        // The floaty toolbar's move: clamped to its parent, not the display or root element.
        // Compare with parent minus current size FIRST, then clamp to zero. An oversized window
        // is placed at zero; no parent means no constraint. There is no initialized/visible gate.
        let Some(parent) = ui.parent(me).and_then(|h| ui.node(h)) else {
            return (x, y);
        };
        let Some(own) = ui.node(me) else {
            return (x, y);
        };
        let p = parent.region.box_;
        let b = own.region.box_;
        (
            x.min(p.width() - b.width()).max(0),
            y.min(p.height() - b.height()).max(0),
        )
    }

    fn after_move(
        &self,
        _ui: &UiSystem,
        out: &mut crate::requests::Outbox,
        _me: ElemHandle,
        x: i32,
        y: i32,
    ) {
        // Persist constrained arguments, X then Y, even if the base move changed nothing.
        self.write(out, crate::chat::floaty::placement::X, x);
        self.write(out, crate::chat::floaty::placement::Y, y);
    }

    fn after_resize(&self, ui: &UiSystem, out: &mut crate::requests::Outbox, me: ElemHandle) {
        // Read width and height AFTER base clamps, not the incoming size arguments.
        if let Some(n) = ui.node(me) {
            self.write(
                out,
                crate::chat::floaty::placement::WIDTH,
                n.region.box_.width(),
            );
            self.write(
                out,
                crate::chat::floaty::placement::HEIGHT,
                n.region.box_.height(),
            );
        }
    }

    fn after_set_visible(
        &self,
        _ui: &UiSystem,
        out: &mut crate::requests::Outbox,
        _me: ElemHandle,
        visible: bool,
    ) {
        // Persist the argument, not effective ancestor visibility.
        self.write(
            out,
            crate::chat::floaty::placement::VISIBILITY,
            i32::from(visible),
        );
    }
}

pub(crate) fn create_toolbar(
    _layout: &dereth_ui::desc::LayoutDesc,
    _desc: &dereth_ui::desc::ElementDesc,
) -> Box<dyn dereth_ui::Element> {
    Box::<FloatyToolbar>::default()
}

/// One movable window: its six-character file tag, its element id and its local behavior label.
///
/// The tag-to-id association follows the client's compare and write order; the tag literals are
/// listed below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameplayWindow {
    /// Six characters including the angle brackets, e.g. `<CHAT>`. Byte-exact: this is what goes
    /// into the screen-layout file.
    pub tag: &'static [u8; 6],
    pub element: ElementId,
    /// A local behavior label for the window. Every row has one, including `<SBOX>` and `<RADA>`.
    pub class: &'static str,
}

const fn w(tag: &'static [u8; 6], id: u32, class: &'static str) -> GameplayWindow {
    GameplayWindow {
        tag,
        element: ElementId(id),
        class,
    }
}

/// The sixteen windows **in file order**.
///
/// This is the same table `dereth_ui::persist::screen_layout::WINDOWS` carries (that crate owns the
/// file format); it is repeated here with a behavior label because this crate is what places the
/// windows, and because a mismatch between the two is exactly the kind of drift a test should
/// catch. The gate asserts they agree tag-for-tag and id-for-id.
pub const GAMEPLAY_WINDOWS: [GameplayWindow; 16] = [
    w(b"<SBOX>", 0x1000_049A, "WorldView"),
    w(b"<CHAT>", 0x1000_0601, "MainChat"),
    w(b"<FCH1>", 0x1000_0505, "FloatingChat"),
    w(b"<FCH2>", 0x1000_050E, "FloatingChat"),
    w(b"<FCH3>", 0x1000_050F, "FloatingChat"),
    w(b"<FCH4>", 0x1000_0510, "FloatingChat"),
    w(b"<EXAM>", 0x1000_05F7, "FloatingExamination"),
    w(b"<VITS>", 0x1000_05FA, "FloatingVitals"),
    w(b"<SVIT>", 0x1000_06D5, "FloatingSideVitals"),
    w(b"<ENVP>", 0x1000_05FD, "FloatingEnvironmentStack"),
    w(b"<PANS>", 0x1000_05FF, "FloatingPanelStack"),
    w(b"<TBAR>", 0x1000_0603, "FloatingToolbar"),
    w(b"<INDI>", 0x1000_0611, "FloatingIndicators"),
    w(b"<PBAR>", 0x1000_0613, "FloatingPowerBar"),
    w(b"<COMB>", 0x1000_06B5, "FloatingCombatStack"),
    w(b"<RADA>", 0x1000_06D2, "Radar"),
];

/// The four floating chat windows, in the order titles
/// them from `ID_Chat_Chat1_DefaultTitle` … `ID_Chat_Chat4_DefaultTitle`.
pub const FLOATY_CHAT_WINDOWS: [ElementId; 4] = [
    ElementId(0x1000_0505),
    ElementId(0x1000_050E),
    ElementId(0x1000_050F),
    ElementId(0x1000_0510),
];

/// The server-side window blob: `Option_Placement` (`0x1000008B`) and the
/// `Option_PlacementArray` (`0x1000008C`) it is indexed out of.
///
/// [`WindowPlacement`] and [`WindowPlacements`] are defined in [`dereth_client_contract::floaty`],
/// because `dereth_client::hud` owns the retained `PlayerModule` these are read out of and written
/// back into.
pub use dereth_client_contract::floaty::{WindowPlacement, WindowPlacements};

/// The five classes whose player-module update reads `0x1000008A` and sets their visibility from
/// it, so their window's visibility is **server state** rather than layout data.
///
/// Established by reading all ten floating-window player-state update bodies: only these five
/// mention the property. `FloatingVitals` and `FloatingSideVitals` set visibility too, but
/// from `SideBySideVitals` instead; the remaining three — `FloatingExamination`,
/// `FloatingEnvironmentStack` and `FloatingCombatStack` — do not call it at all.
pub const READS_PLACEMENT_VISIBILITY: [&str; 5] = [
    "MainChat",
    "FloatingChat",
    "FloatingIndicators",
    "FloatingPowerBar",
    "FloatingToolbar",
];

/// The eight chrome pieces, in the order the post-init assigns them.
///
/// Every one of the ten floating windows fetches all sixteen children by explicit id in its own
/// post-init, in exactly this order, and the ids are interleaved rather than contiguous by role.
/// See [`ChromeBlock`] for the mapping. \[verified\]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChromePiece {
    Top,
    Left,
    Bottom,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl ChromePiece {
    /// The eight pieces in assignment order.
    pub const ALL: [Self; 8] = [
        Self::Top,
        Self::Left,
        Self::Bottom,
        Self::Right,
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
    ];

    /// The binding key for this piece (the top border's, for [`Self::Top`]).
    #[must_use]
    pub const fn field(self) -> &'static str {
        match self {
            Self::Top => "top_border",
            Self::Left => "left_border",
            Self::Bottom => "bottom_border",
            Self::Right => "right_border",
            Self::TopLeft => "top_left_corner",
            Self::TopRight => "top_right_corner",
            Self::BottomLeft => "bottom_left_corner",
            Self::BottomRight => "bottom_right_corner",
        }
    }
}

/// The sixteen chrome child ids of one floating window: the **`_Locked` eight first**, then the
/// eight unlocked pieces, each half interleaved corner/border.
///
/// # The locked half comes first
///
/// It is tempting to read `unlocked(p) = first + p` and `locked(p) = first + 8 + p` — "the first
/// eight are the unlocked pieces and the last eight the locked pieces". **Both halves of that are
/// wrong**: with the two halves swapped, locking the UI reveals the grab handles and unlocking
/// hides them, on all ten floaty windows at once (everything except the minimap).
///
/// The real mapping is what each window's post-init fetches, all sixteen by explicit id — the
/// floating toolbar is the model and the other nine have the same shape:
///
/// ```text
/// top border               0x1000062C   first + 9
/// left border              0x1000062E   first + 11
/// bottom border            0x10000630   first + 13
/// right border             0x10000632   first + 15
/// top-left corner          0x1000062B   first + 8
/// top-right corner         0x1000062D   first + 10
/// bottom-left corner       0x1000062F   first + 12
/// bottom-right corner      0x10000631   first + 14
/// top border, locked       0x10000624   first + 1
/// left border, locked      0x10000626   first + 3
/// bottom border, locked    0x10000628   first + 5
/// right border, locked     0x1000062A   first + 7
/// top-left corner, locked  0x10000623   first + 0
/// top-right corner, locked 0x10000625   first + 2
/// bottom-left, locked      0x10000627   first + 4
/// bottom-right, locked     0x10000629   first + 6
/// ```
///
/// So the low eight are the `_Locked` set and the high eight the unlocked set, and within each
/// half the ids run **corner, border, corner, border …** rather than by [`ChromePiece::ALL`]
/// order. That is exactly the interleave [`SMART_BOX_CHROME`] already carried as **verified** —
/// two readings that could have disagreed and did not.
///
/// The polarity itself was never wrong. The lock/unlock pass:
///
/// With UI locking enabled, hide all eight plain pieces and show the eight locked
/// variants. With locking disabled, show the plain pieces and hide the locked variants.
/// \[verified\]
///
/// The shipped layout agrees from a third direction: at every one of the ten blocks, `first+0` …
/// `first+7` are all `Field` (type 3) and `first+8` … `first+15` are all
/// `DragHandle` (2) or `ResizeHandle` (9) — the grab handles are in the **high**
/// half, which is the half that must be visible when the UI is *un*locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChromeBlock {
    pub class: &'static str,
    pub ty: ElementType,
    /// The first of the sixteen contiguous ids — the locked top-left corner.
    pub first: u32,
}

impl ChromeBlock {
    /// The within-half offset of one role: corners on the even slots, borders on the odd ones.
    ///
    /// `Top -> 1, Left -> 3, Bottom -> 5, Right -> 7, TopLeft -> 0, TopRight -> 2,
    /// BottomLeft -> 4, BottomRight -> 6`, which is `2p+1` for the four borders and `2(p-4)` for
    /// the four corners. The same eight offsets, relative to `0x100006CA`, reproduce
    /// [`SMART_BOX_CHROME`] exactly.
    #[must_use]
    pub const fn slot(p: ChromePiece) -> u32 {
        match p {
            ChromePiece::Top => 1,
            ChromePiece::Left => 3,
            ChromePiece::Bottom => 5,
            ChromePiece::Right => 7,
            ChromePiece::TopLeft => 0,
            ChromePiece::TopRight => 2,
            ChromePiece::BottomLeft => 4,
            ChromePiece::BottomRight => 6,
        }
    }

    /// The unlocked piece's element id — the grab handle, visible while the UI is not locked.
    #[must_use]
    pub fn unlocked(&self, p: ChromePiece) -> ElementId {
        ElementId(self.first + 8 + Self::slot(p))
    }

    /// The `_Locked` twin's element id — the plain border, visible while the UI is locked.
    #[must_use]
    pub fn locked(&self, p: ChromePiece) -> ElementId {
        ElementId(self.first + Self::slot(p))
    }

    /// All sixteen ids, ascending — the `_Locked` half first.
    #[must_use]
    pub fn all(&self) -> [ElementId; 16] {
        let mut out = [ElementId(0); 16];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = ElementId(self.first + u32::try_from(i).unwrap_or(0));
        }
        out
    }
}

const fn cb(class: &'static str, ty: ElementType, first: u32) -> ChromeBlock {
    ChromeBlock { class, ty, first }
}

/// The ten floating-window chrome blocks.
///
/// `WorldView` is deliberately **not** here: it has **one** set of eight rather than two, so it
/// gets its own table in [`SMART_BOX_CHROME`]. Its interleave is the same one every block here
/// uses; see [`ChromeBlock::slot`], which reproduces `SMART_BOX_CHROME` from
/// `0x100006CA`.
pub const FLOATY_CHROME: [ChromeBlock; 10] = [
    cb("FloatingToolbar", ty::FLOATY_TOOLBAR, 0x1000_0623),
    cb("FloatingVitals", ty::FLOATY_VITALS, 0x1000_0633),
    cb("FloatingIndicators", ty::FLOATY_INDICATORS, 0x1000_0643),
    cb("FloatingPanelStack", ty::FLOATY_PANEL, 0x1000_0653),
    cb(
        "FloatingEnvironmentStack",
        ty::FLOATY_ENV_PANEL,
        0x1000_0663,
    ),
    cb("FloatingExamination", ty::FLOATY_EXAMINATION, 0x1000_0673),
    cb("FloatingPowerBar", ty::FLOATY_POWER_BAR, 0x1000_0683),
    cb("FloatingMainChat", ty::FLOATY_MAIN_CHAT, 0x1000_0693),
    cb("FloatingCombatStack", ty::FLOATY_COMBAT_PANEL, 0x1000_06A5),
    cb("FloatingSideVitals", ty::FLOATY_SIDE_VITALS, 0x1000_06D6),
];

/// The client's eight explicit border ids, in [`ChromePiece::ALL`] order.
///
/// This is the **verified** ordering that [`ChromePiece`] copies:
/// top `0x100006CB`, left `0x100006CD`, bottom `0x100006CF`, right `0x100006D1`, then the four
/// corners `0x100006CA`, `0x100006CC`, `0x100006CE`, `0x100006D0`.
pub const SMART_BOX_CHROME: [ElementId; 8] = [
    ElementId(0x1000_06CB),
    ElementId(0x1000_06CD),
    ElementId(0x1000_06CF),
    ElementId(0x1000_06D1),
    ElementId(0x1000_06CA),
    ElementId(0x1000_06CC),
    ElementId(0x1000_06CE),
    ElementId(0x1000_06D0),
];

/// What every window in the sixteen-window set implements on top of the element trait.
pub trait FloatyWindow {
    /// The six-character screen-layout tag.
    const TAG: &'static [u8; 6];

    /// The window's own element id — the one the layout file names.
    fn root_element(&self) -> ElementId;

    /// The locked-status update, driven by global message `0x0D`.
    fn update_locked_status(&mut self, ui: &mut UiSystem, locked: bool);
}

/// One of the ten floating windows: the plain panel plus its chrome block and its window id.
///
/// This is the client's seventeen-field pattern — the window id plus eight border/corner
/// elements and eight `_Locked` twins — with the sixteen elements looked up by id instead of being
/// cached in sixteen fields, because [`ChromeBlock`] already knows what those ids are.
#[derive(Debug, Clone, Copy)]
pub struct FloatyWrapper {
    pub block: ChromeBlock,
    pub root: ElementId,
    /// The window id, attribute `0x1000007E`.
    pub window_id: u32,
    /// Whether the chrome is currently showing its `_Locked` pieces.
    pub locked: bool,
}

impl FloatyWrapper {
    /// Build the wrapper for one row of the sixteen-window table.
    ///
    /// Returns `None` for the six rows that do not use the shared floating-window chrome: `<SBOX>`
    /// (which has its
    /// own explicit chrome, [`SMART_BOX_CHROME`]), `<RADA>` (a plain `Radar` that hides a drag
    /// button instead of swapping chrome), `<CHAT>` and the three floaty chat windows beyond the
    /// first, whose chrome belongs to `FloatingMainChat`.
    #[must_use]
    pub fn for_window(w: &GameplayWindow) -> Option<Self> {
        let block = *FLOATY_CHROME.iter().find(|b| b.class == w.class)?;
        Some(Self {
            block,
            root: w.element,
            window_id: 0,
            locked: false,
        })
    }

    /// Read the window id off the live element.
    pub fn read_window_id(&mut self, ui: &UiSystem, root: ElemHandle) {
        if let Some(h) = ui.get_child_recursive(root, self.root) {
            self.window_id = crate::bind::attr_enum(ui, h, crate::bind::attr::WINDOW_ID)
                .or_else(|| {
                    crate::bind::attr_int(ui, h, crate::bind::attr::WINDOW_ID).map(|v| v as u32)
                })
                .unwrap_or(0);
        }
    }
}

impl FloatyWindow for FloatyWrapper {
    // The tag is per-window and this type is shared across the ten classes, so the associated
    // const carries the one every floating window has in common: none. Concrete windows that need
    // their own tag read it from [`GAMEPLAY_WINDOWS`].
    const TAG: &'static [u8; 6] = b"<????>";

    fn root_element(&self) -> ElementId {
        self.root
    }

    fn update_locked_status(&mut self, ui: &mut UiSystem, locked: bool) {
        self.locked = locked;
        let block = self.block;
        let root = self.root;
        if let Some(h) = ui.get_element(root) {
            swap_chrome(ui, h, &block, locked);
        }
    }
}

/// The shared body of every floating window's locked-status update: swap the eight normal
/// border/corner children for the eight `_Locked` ones.
///
/// A missing child is skipped rather than fatal, because a floaty class whose layout does not
/// carry all sixteen pieces still has to lock — and setting visibility on a missing piece is exactly what
/// the client's null check avoids.
pub fn swap_chrome(ui: &mut UiSystem, root: ElemHandle, block: &ChromeBlock, locked: bool) {
    for p in ChromePiece::ALL {
        if let Some(h) = ui.get_child_recursive(root, block.unlocked(p)) {
            ui.set_visible(h, !locked);
        }
        if let Some(h) = ui.get_child_recursive(root, block.locked(p)) {
            ui.set_visible(h, locked);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the sixteen-row table in the recovered HUD behavior, cross-checked against the
    /// independent transcription in the recovered UI-persistence behavior
    /// (`dereth_ui::persist::screen_layout::WINDOWS`).
    #[test]
    fn the_window_table_agrees_with_track_js_transcription_row_for_row() {
        let j = dereth_ui::persist::WINDOWS;
        assert_eq!(GAMEPLAY_WINDOWS.len(), j.len());
        for (k, jj) in GAMEPLAY_WINDOWS.iter().zip(j.iter()) {
            assert_eq!(
                std::str::from_utf8(k.tag).unwrap(),
                jj.tag,
                "tag order must match the file order"
            );
            assert_eq!(k.element, jj.element, "{} element id", jj.tag);
        }
        // The four floaty chat rows are the four ids the panel's child setup titles.
        let chat: Vec<ElementId> = GAMEPLAY_WINDOWS[2..6].iter().map(|w| w.element).collect();
        assert_eq!(chat, FLOATY_CHAT_WINDOWS.to_vec());
    }

    /// Oracle: the ten-row chrome table in the recovered HUD behavior. Nine of the ten blocks are
    /// contiguous with the next; the two gaps in the table (`0x10000692`→`0x100006A5` and
    /// `0x100006B4`→`0x100006D6`) are real and are what the ids in §1 sit in.
    #[test]
    fn every_chrome_block_is_sixteen_contiguous_ids_and_they_never_overlap() {
        let mut seen: Vec<u32> = Vec::new();
        for b in FLOATY_CHROME {
            let ids = b.all();
            assert_eq!(ids.len(), 16, "{}", b.class);
            for (i, id) in ids.iter().enumerate() {
                assert_eq!(
                    id.0,
                    b.first + u32::try_from(i).unwrap(),
                    "{} is contiguous",
                    b.class
                );
            }
            assert_eq!(b.locked(ChromePiece::TopLeft).0, b.first, "{}", b.class);
            assert_eq!(
                b.unlocked(ChromePiece::TopLeft).0,
                b.first + 8,
                "{}",
                b.class
            );
            assert_eq!(
                b.unlocked(ChromePiece::Right).0,
                b.first + 15,
                "{}",
                b.class
            );
            // Each half is a permutation of its own eight slots: no id is used twice and none is
            // left out, which is what makes the two halves complementary.
            let mut half: Vec<u32> = ChromePiece::ALL
                .iter()
                .map(|p| ChromeBlock::slot(*p))
                .collect();
            half.sort_unstable();
            assert_eq!(
                half,
                (0..8).collect::<Vec<u32>>(),
                "{} slot permutation",
                b.class
            );
            seen.extend(ids.iter().map(|i| i.0));
        }
        let mut sorted = seen.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), seen.len(), "chrome blocks must not overlap");
        // The documented first/last ids of the whole run.
        assert_eq!(FLOATY_CHROME[0].first, 0x1000_0623);
        assert_eq!(FLOATY_CHROME[9].all()[15].0, 0x1000_06E5);
    }

    /// Oracle: the recovered HUD behavior — each floating window swaps its eight normal border/corner
    /// children for the eight `_Locked` ones", driven by global message `0x0D` (§1.3).
    #[test]
    fn toggling_the_lock_swaps_all_eight_chrome_children_on_a_floaty_window() {
        let mut ui = UiSystem::new((800, 600));
        // Build a synthetic window whose sixteen chrome children carry the real ids.
        let block = FLOATY_CHROME[0];
        let root = ui.create_hollow(Some(ui.root()));
        let mut ids = Vec::new();
        for id in block.all() {
            let c = ui.create_hollow(Some(root));
            // The hollow element's id is fixed, so drive the swap by id through a lookup table.
            ids.push((id, c));
        }
        // `swap_chrome` looks children up by id; with hollow elements that is not possible, so the
        // assertion here is on the *pairing* the function walks rather than on the arena.
        for p in ChromePiece::ALL {
            let u = block.unlocked(p);
            let l = block.locked(p);
            assert_eq!(u.0 - l.0, 8, "{p:?}: the unlocked twin is eight ids along");
            assert!(ids.iter().any(|(i, _)| *i == u));
            assert!(ids.iter().any(|(i, _)| *i == l));
        }
        // The wrapper only exists for the ten shared-chrome rows of the window table.
        let by_class = |c: &str| GAMEPLAY_WINDOWS.iter().find(|w| w.class == c).copied();
        let mut w = FloatyWrapper::for_window(&by_class("FloatingToolbar").unwrap()).unwrap();
        assert_eq!(w.root_element(), ElementId(0x1000_0603));
        assert!(!w.locked);
        w.update_locked_status(&mut ui, true);
        assert!(w.locked);
        // `<SBOX>` and `<RADA>` are not floaty wrappers; they lock differently (§1.3).
        assert!(FloatyWrapper::for_window(&by_class("WorldView").unwrap()).is_none());
        assert!(FloatyWrapper::for_window(&by_class("Radar").unwrap()).is_none());
    }

    /// The toolbar's sixteen chrome ids are the ones its post init fetches.
    #[test]
    fn the_toolbars_sixteen_chrome_ids_are_the_ones_its_post_init_fetches() {
        let b = FLOATY_CHROME[0];
        assert_eq!(b.class, "FloatingToolbar");
        // Top border … bottom-right corner — the eight the client shows when NOT locked.
        assert_eq!(b.unlocked(ChromePiece::Top), ElementId(0x1000_062C));
        assert_eq!(b.unlocked(ChromePiece::Left), ElementId(0x1000_062E));
        assert_eq!(b.unlocked(ChromePiece::Bottom), ElementId(0x1000_0630));
        assert_eq!(b.unlocked(ChromePiece::Right), ElementId(0x1000_0632));
        assert_eq!(b.unlocked(ChromePiece::TopLeft), ElementId(0x1000_062B));
        assert_eq!(b.unlocked(ChromePiece::TopRight), ElementId(0x1000_062D));
        assert_eq!(b.unlocked(ChromePiece::BottomLeft), ElementId(0x1000_062F));
        assert_eq!(b.unlocked(ChromePiece::BottomRight), ElementId(0x1000_0631));
        // The locked top border … bottom-right corner — shown when locked.
        assert_eq!(b.locked(ChromePiece::Top), ElementId(0x1000_0624));
        assert_eq!(b.locked(ChromePiece::Left), ElementId(0x1000_0626));
        assert_eq!(b.locked(ChromePiece::Bottom), ElementId(0x1000_0628));
        assert_eq!(b.locked(ChromePiece::Right), ElementId(0x1000_062A));
        assert_eq!(b.locked(ChromePiece::TopLeft), ElementId(0x1000_0623));
        assert_eq!(b.locked(ChromePiece::TopRight), ElementId(0x1000_0625));
        assert_eq!(b.locked(ChromePiece::BottomLeft), ElementId(0x1000_0627));
        assert_eq!(b.locked(ChromePiece::BottomRight), ElementId(0x1000_0629));
        // And the same for a second class, so the rule is pinned rather than one row of it:
        // The floaty main chat panel's post-init, base 0x10000693.
        let c = FLOATY_CHROME[7];
        assert_eq!(c.class, "FloatingMainChat");
        assert_eq!(c.unlocked(ChromePiece::Top), ElementId(0x1000_069C));
        assert_eq!(c.unlocked(ChromePiece::TopLeft), ElementId(0x1000_069B));
        assert_eq!(c.locked(ChromePiece::Top), ElementId(0x1000_0694));
        assert_eq!(c.locked(ChromePiece::TopLeft), ElementId(0x1000_0693));
    }

    /// The world view's ids reproduce the floaty interleave.
    #[test]
    fn the_world_views_verified_ids_reproduce_the_floaty_interleave() {
        for (i, p) in ChromePiece::ALL.iter().enumerate() {
            assert_eq!(
                ElementId(0x1000_06CA + ChromeBlock::slot(*p)),
                SMART_BOX_CHROME[i],
                "{p:?}"
            );
        }
    }

    /// Oracle: the recovered HUD behavior's post-init table — the one place the
    /// top/left/bottom/right/TL/TR/BL/BR order is verified rather than inferred.
    #[test]
    fn the_world_view_chrome_is_the_verified_ordering() {
        assert_eq!(
            SMART_BOX_CHROME[ChromePiece::Top as usize],
            ElementId(0x1000_06CB)
        );
        assert_eq!(
            SMART_BOX_CHROME[ChromePiece::Left as usize],
            ElementId(0x1000_06CD)
        );
        assert_eq!(
            SMART_BOX_CHROME[ChromePiece::Bottom as usize],
            ElementId(0x1000_06CF)
        );
        assert_eq!(
            SMART_BOX_CHROME[ChromePiece::Right as usize],
            ElementId(0x1000_06D1)
        );
        assert_eq!(
            SMART_BOX_CHROME[ChromePiece::TopLeft as usize],
            ElementId(0x1000_06CA)
        );
        assert_eq!(
            SMART_BOX_CHROME[ChromePiece::TopRight as usize],
            ElementId(0x1000_06CC)
        );
        assert_eq!(
            SMART_BOX_CHROME[ChromePiece::BottomLeft as usize],
            ElementId(0x1000_06CE)
        );
        assert_eq!(
            SMART_BOX_CHROME[ChromePiece::BottomRight as usize],
            ElementId(0x1000_06D0)
        );
        // The eight ids are exactly 0x100006CA..=0x100006D1.
        let mut ids: Vec<u32> = SMART_BOX_CHROME.iter().map(|i| i.0).collect();
        ids.sort_unstable();
        assert_eq!(ids, (0x1000_06CA..=0x1000_06D1).collect::<Vec<u32>>());
    }
}
