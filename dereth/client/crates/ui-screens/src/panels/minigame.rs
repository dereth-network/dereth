//! The mini-game panel — the **chess window**: an 8×8 board, three buttons, and five messages
//! they send.
//!
//! Five pieces make the window, and this module joins them:
//!
//! | piece | where |
//! |---|---|
//! | the open-window notice | `use_object`'s `UseResult::BeginGame` arm emits `Notice::BeginGame(board)` |
//! | the chess rules engine | `dereth_client_model::chess` |
//! | the five outbound opcodes | `dereth_protocol::trade` encoders, sent through [`UiRequest`] |
//! | the six inbound opcodes | decoded in `dereth_protocol::trade` and received by `Hud::ui_event` |
//! | the mini-game panel | an element type, a shipped layout, a `panels::catalogue` row and this module |
//!
//! # What raises this window
//!
//! Not a message. The item holder's "determine use result" answers **7** for an object whose
//! `ItemType` carries `TYPE_GAMEPIECE` (`0x80000000`, the sign bit) and
//! which the player does not own; the player system's "using item" case 7 is
//! a begin-game notice; the mini-game panel's handler performs only
//! visibility and join-game actions. The **first thing the raised
//! window does** is send `0x0269 Game_Join { game_id = the board's guid, which_team = -1 }`.
//! See `dereth_client_model::minigame`'s header for the whole chain.
//!
//! # The shipped tree, measured
//!
//! The mini-game panel instance is **`0x10000188`** — a `<PANS>` page, and the mini-game lamp
//! `0x100000F3` opens through input action `0x1000000A` (`screens::gameplay`'s lamp table).
//! Panel initialization binds the resign button `0x10000175` visible and the following children;
//! board construction binds the list box:
//!
//! | id | what | the post-init's first write |
//! |---|---|---|
//! | `0x10000174` | board list box | 64 rows from template `0x10000178` |
//! | `0x10000176` | Pass button | starts **hidden** |
//! | `0x10000177` | Stalemate button | starts visible |
//!
//! # Two corrections to `panels::catalogue`
//!
//! 1. **The cell template is `0x10000178`, not `0x10000179`.** The client adds sixty-four rows
//!    from the template with that element id, in a loop of 64. `MINI_GAME_TEMPLATES` carried
//!    `0x10000179`.
//! 2. **It switches on two element messages, not one.** The handler tests
//!    message 1 and message `0x1C`; the catalogue row
//!    carried `&[1]`. The `0x1C` arm is the board itself, and without it no move can ever be made.
//!
//! # The picture
//!
//! The game board grid draws media images in mode 3
//! over the 64 rows and reads
//! twelve icon `DataId`s from layout property **`0x10000021 UI_MiniGame_PieceIconArray`**. This panel
//! paints the live cells through `UiSystem::set_media_image`, including draw mode 3's Alpha4
//! path. Empty squares are hidden. Retail's draw has no selected-square highlight, so neither does
//! this panel.
//!
//! # Shard safety
//!
//! **No datagram leaves this process.** Every gesture here appends a [`UiRequest`] to the
//! thread-local outbox; the host is what turns one into bytes.

use dereth_primitives::DataId;
use dereth_ui::{ElemHandle, ElementId, ElementMessage, PropertyValue, UiSystem};

use crate::view::{GameView, UiRequest};

/// The element class the client registers for this panel, and what
/// the element-type answer returns.
pub const ELEMENT_CLASS: u32 = 0x1000_001E;

/// The mini-game panel instance in the shipped `0x21000005` tree.
pub const PANEL: ElementId = ElementId(0x1000_0188);

/// The piece list box — the board grid's child `0x10000174`.
pub const BOARD: ElementId = ElementId(0x1000_0174);
/// The resign button — the panel's child `0x10000175`.
pub const RESIGN_BUTTON: ElementId = ElementId(0x1000_0175);
/// The pass button.
pub const PASS_BUTTON: ElementId = ElementId(0x1000_0176);
/// The stalemate button.
pub const STALEMATE_BUTTON: ElementId = ElementId(0x1000_0177);
/// The cell template the 64 rows come from. See correction 1.
pub const CELL_TEMPLATE: ElementId = ElementId(0x1000_0178);

/// `UI_MiniGame_PieceIconArray`, read from [`BOARD`] when resolving a piece icon.
pub const PIECE_ICON_ARRAY: u32 = 0x1000_0021;

/// `0x40` — the loop count, and the number of squares on a chessboard.
pub const CELLS: usize = 64;

/// The client's first message parameter `== 7` — the left **press**.
///
/// Not the salvage panel's double-click (`10`) and not a click: a chess move is made on the
/// press, which is why the board's arm is message `0x1C` and not `1`.
pub const PRESS_ACTION: u32 = 7;

/// The mini-game panel, bound to a live tree.
///
/// It holds **no game state**: team, current game, state, stalemate, and the
/// board all live on `dereth_client_model::minigame::MiniGame`, because the six inbound opcodes arrive at
/// the message layer and the local rules engine has to be the same board for both halves. This is
/// the view of it — the same division `panels::house` has.
#[derive(Debug, Default)]
pub struct MiniGamePanel {
    /// The mini-game element — [`PANEL`], whose visibility this panel controls.
    pub root: Option<ElemHandle>,
    /// The piece list box.
    pub board: Option<super::listbox::ListBoxWidget>,
    /// The resign button.
    pub resign: Option<ElemHandle>,
    /// The pass button.
    pub pass: Option<ElemHandle>,
    /// The stalemate button.
    pub stalemate: Option<ElemHandle>,
    /// Whether the last [`Self::update`] left the window visible.
    pub visible: bool,
    /// The list box's twelve `UI_MiniGame_PieceIconArray` entries, in property order.
    pub piece_icons: [Option<DataId>; 12],

    // ---- counters, three-state on purpose ----------------------------------------------------
    /// Cells the board actually created. **64 or the binding is wrong**, and zero is the shape a
    /// missing template id produces — which is exactly what correction 1 in this module's header
    /// was found by.
    pub cells_built: usize,
    /// Presses that resolved to a square and were handed to the model.
    pub presses_routed: u32,
    /// Presses inside the board that resolved to no row at all.
    pub presses_missed: u32,
    /// Button clicks this panel took.
    pub button_clicks: u32,
    /// Visibility edges this panel wrote.
    pub visibility_changes: u32,
    /// Board-redraw edges this panel has consumed; each one repaints the board.
    ///
    /// Note what is **not** invented here: retail's board draw reads the pieces and nothing else —
    /// it does not highlight the selected square — so this panel does not either. A selection
    /// highlight would be more than retail does.
    pub redraws_owed: u32,
    /// The `draws` this panel last saw, so the counter above moves on an edge.
    last_draws: Option<u32>,
}

impl MiniGamePanel {
    /// The mini game panel's post-init plus the game board grid's constructor, minus
    /// the nine notice-handler registrations and the twelve piece-icon lookups.
    ///
    /// Original initialization binds three buttons:
    ///
    /// * resign at `0x10000175`;
    /// * pass at `0x10000176`;
    /// * stalemate at `0x10000177`.
    ///
    /// It shows resign and stalemate; pass starts hidden.
    /// It registers nine notice handlers: begin game, try to quit the game and end game, plus the
    /// six game-traffic notices (join-game response, start game, move response, opponent turn,
    /// opponent offers stalemate, game over).
    ///
    /// It then creates the board grid and binds its piece list at `0x10000174`.
    /// The list receives 64 rows from template `0x10000178`.
    ///
    /// Bound off the screen **root**, like every other `<PANS>` page: `0x10000188` is unique in the
    /// shipped tree and the child lookup is recursive. The window is then hidden, because the
    /// panel stack's start visibility leaves every page down and only the begin-game notice
    /// raises this one.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        *self = Self::default();
        let Some(page) = ui.get_child_recursive(root, PANEL) else {
            return;
        };
        // Confirm the id is the class the client registered, so a layout change that moved the
        // panel reads as unbound rather than as bound to the wrong element.
        if ui.node(page).is_some_and(|n| n.ty().0 != ELEMENT_CLASS) {
            return;
        }
        self.root = Some(page);
        self.resign = ui.get_child_recursive(page, RESIGN_BUTTON);
        self.pass = ui.get_child_recursive(page, PASS_BUTTON);
        self.stalemate = ui.get_child_recursive(page, STALEMATE_BUTTON);
        if let Some(h) = self.resign {
            ui.set_visible(h, true);
        }
        // The client hides it. The Pass button is hidden until a game is in progress.
        if let Some(h) = self.pass {
            ui.set_visible(h, false);
        }
        if let Some(h) = self.stalemate {
            ui.set_visible(h, true);
        }
        if let Some(list) = ui.get_child_recursive(page, BOARD) {
            self.piece_icons = piece_icon_array(ui, list);
            let mut w = super::listbox::ListBoxWidget::bind(ui, list);
            // The client resolves the row out of the list's own
            // template list **by element id**; this crate's widget indexes that list, so the
            // id is turned into an index here rather than a second lookup path being invented.
            if let Some(i) = w.templates.iter().position(|(_, e)| *e == CELL_TEMPLATE) {
                for _ in 0..CELLS {
                    if w.add_from_template(ui, i, None).is_none() {
                        break;
                    }
                }
            }
            w.update_layout(ui);
            self.cells_built = w.items.len();
            self.board = Some(w);
        }
        ui.set_visible(page, false);
    }

    /// True once the board list box was found and filled — the binding without which no move can
    /// be made.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.cells_built == CELLS
    }

    /// Mirror the panel and Pass-button visibility once per frame.
    ///
    /// Retail pushes: the begin-game notice raises the window and the end-game notice lowers it,
    /// both synchronously. This build has no notice bus at this seam, so the model's `visible` is
    /// the authority and this is the pull that applies it — the same arrangement
    /// [`super::house::HousePanel`] uses, and observationally identical because the only writers
    /// of that flag are those two handlers.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) {
        let Some(g) = view.minigame() else { return };
        if g.visible != self.visible {
            self.set_visible(ui, g.visible);
        }
        if self.last_draws != Some(g.draws) {
            self.last_draws = Some(g.draws);
            self.redraws_owed += 1;
            if let Some(board) = self.board.as_ref() {
                for (cell, &h) in board.items.iter().take(CELLS).enumerate() {
                    let did = g.piece_slots[cell]
                        .and_then(|slot| self.piece_icons.get(usize::from(slot)))
                        .copied()
                        .flatten();
                    if let Some(did) = did {
                        ui.set_media_image(h, did, 3);
                        ui.set_visible(h, true);
                    } else {
                        ui.set_visible(h, false);
                    }
                }
            }
        }
    }

    /// The Pass button is hidden during initialization, and **nothing else in the client
    /// shows it**.
    ///
    /// Across the mini-game panel and the board grid, the only writes to the Pass button are
    /// initialization's binding and that hide. So the client ships a Pass button
    /// that is never visible, and the `0x026D Game_MovePass` its message-handler arm
    /// sends is unreachable by mouse. The arm is still reproduced — it is reachable in principle
    /// and a show may yet be found — and this constant is here so the finding is not
    /// rediscovered as a bug in this panel.
    pub const PASS_BUTTON_IS_NEVER_SHOWN: bool = true;

    /// Show or hide the window itself.
    pub fn set_visible(&mut self, ui: &mut UiSystem, visible: bool) {
        self.visible = visible;
        self.visibility_changes += 1;
        if let Some(h) = self.root {
            ui.set_visible(h, visible);
        }
    }

    /// The mini game panel's element-message handler — **both** arms.
    ///
    /// On message 1: **Resign** (`0x10000175`) opens the quit-game confirmation dialog unless one
    /// is already up; **Pass** (`0x10000176`) sends the move-pass event when a game is in
    /// progress, otherwise shows `"You are not currently playing a game."` on chat channel `0x1A`;
    /// **Stalemate** (`0x10000177`) toggles the stalemate flag and sends it when a game is in
    /// progress, otherwise the same refusal. On message `0x1C`, when there is a board, the board
    /// handles the mouse press.
    ///
    /// The decisions are all `dereth_client_model::minigame::MiniGame`'s, because every one of them reads
    /// the current game or the board and neither lives here. This function's whole job is the
    /// **gate** — the three ids, the press action, and "the press was inside my own list box" —
    /// which is what the element base's broadcast subtree bubble does in the
    /// client and what `RemainingPanels::on_element_message` makes this crate supply by hand.
    pub fn on_element_message(&mut self, ui: &mut UiSystem, m: &ElementMessage) -> bool {
        use dereth_ui::msg::element::id as msg;
        if m.id == msg::BUTTON_CLICKED {
            let id = m.source_id.0;
            if id != RESIGN_BUTTON.0 && id != PASS_BUTTON.0 && id != STALEMATE_BUTTON.0 {
                return false;
            }
            self.button_clicks += 1;
            ui.requests.emit(UiRequest::MiniGameButton(id));
            return true;
        }
        if m.id != msg::MOUSE_PRESS {
            return false;
        }
        // The board arm refuses any source but `0x10000174` and any
        // any `dwParam1` but 7.
        let Some(list) = self.board.as_ref() else {
            return false;
        };
        if !list.owns(ui, m.source) || m.p1 != PRESS_ACTION {
            return false;
        }
        let (mx, my) = ui.mouse_pos();
        match list
            .item_under_mouse(ui, mx, my)
            .and_then(|h| list.index_of(h))
        {
            Some(cell) => {
                self.presses_routed += 1;
                ui.requests.emit(UiRequest::MiniGameBoardPress(cell));
            }
            // The item-under-mouse lookup answered nothing: retail leaves `x`/`y` at `-1` and
            // the square validity check refuses. Counted rather than silent, because "the board took the
            // press and resolved no square" is a different fact from "no press arrived".
            None => self.presses_missed += 1,
        }
        true
    }
}

/// The piece-icon array's DataID at each index, as the board grid reads it.
/// Preserve array positions: a non-DataFile member is an unavailable icon, not permission to
/// shift every following chess piece onto the wrong picture.
fn piece_icon_array(ui: &UiSystem, list: ElemHandle) -> [Option<DataId>; 12] {
    let mut out = [None; 12];
    let Some(PropertyValue::Array(members)) = ui
        .node(list)
        .and_then(|node| node.merged_properties().get(PIECE_ICON_ARRAY).cloned())
    else {
        return out;
    };
    for (slot, member) in members.iter().take(out.len()).enumerate() {
        if let PropertyValue::DataFile(did) = member.value {
            out[slot] = Some(did);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four bound ids and the template are the ones retail uses. A transposition here is
    /// the whole difference between a board and an empty frame, and correction 1 in this module's
    /// header is exactly that mistake made once already.
    #[test]
    fn the_bound_ids_are_the_ones_postinit_and_the_grid_constructor_push() {
        assert_eq!(PANEL.0, 0x1000_0188);
        assert_eq!(BOARD.0, 0x1000_0174);
        assert_eq!(RESIGN_BUTTON.0, 0x1000_0175);
        assert_eq!(PASS_BUTTON.0, 0x1000_0176);
        assert_eq!(STALEMATE_BUTTON.0, 0x1000_0177);
        assert_eq!(CELL_TEMPLATE.0, 0x1000_0178);
        assert_eq!(CELLS, 0x40);
    }
}
