//! The interface's widgets: the kit's ([`dereth_ui_kit::widgets`]), drawn from the interface's
//! own art, with what the interface adds to their context: the item or spell the player has in
//! hand, and where it can be dropped.

use crate::draw::Rect;
use crate::ui::paint::Painter;

pub use dereth_ui_kit::widgets::*;

/// The per-frame context every widget is drawn with: the kit's, and the drag and drop and the
/// game's notices the interface keeps beside it.
#[derive(Debug)]
pub struct Ctx<'a> {
    /// The kit's share: the time, the input, the colours and whether the pointer is over a
    /// widget.
    pub kit: dereth_ui_kit::widgets::Ctx<'a>,
    /// What the player has in hand.
    pub drag: &'a mut Option<crate::ui::Drag>,
    /// Where a drag can land this frame, in drawing order: what a drop there asks for, or
    /// `None` for a window that takes the drop and does nothing with it. The last one drawn
    /// under the pointer wins.
    pub drops: Vec<(Rect, Option<Drop>)>,
    /// The spell keys the game raised since the last frame.
    pub magic: Vec<dereth_client_contract::view::MagicNotice>,
    /// Whether the player is off the ground, which refuses a log-off.
    pub airborne: bool,
}

impl<'a> std::ops::Deref for Ctx<'a> {
    type Target = dereth_ui_kit::widgets::Ctx<'a>;
    fn deref(&self) -> &Self::Target {
        &self.kit
    }
}

impl std::ops::DerefMut for Ctx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.kit
    }
}

/// What a drop asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Drop {
    /// The game's own drop: a slot, a container, the character, the shortcut bar, the world.
    Target(dereth_client_contract::view::DropTarget),
    /// Onto the vendor's sell list.
    Sell,
    /// Onto the secure trade's offer, at this position.
    Trade(u32),
    /// Onto a spell bar's slot: spells only.
    SpellSlot { tab: usize, index: usize },
    /// Onto the salvage window.
    Salvage,
    /// Onto a window that takes what is dropped on it itself.
    Window,
    /// Onto a slot of the pad's cross hotbars, which binds it there.
    CrossSlot(usize),
}

impl From<dereth_client_contract::view::DropTarget> for Drop {
    fn from(t: dereth_client_contract::view::DropTarget) -> Self {
        Self::Target(t)
    }
}

impl Drop {
    /// The request a drop of `item` here raises, if any.
    #[must_use]
    pub fn request(
        self,
        item: dereth_primitives::ObjectId,
    ) -> Option<dereth_client_contract::UiRequest> {
        use dereth_client_contract::UiRequest;
        match self {
            Self::Target(target) => Some(UiRequest::DragDrop { item, target }),
            Self::Sell => Some(UiRequest::VendorAddToSell { item }),
            Self::Trade(position) => Some(UiRequest::TradeAddItem { item, position }),
            Self::Salvage => Some(UiRequest::SalvageList(
                dereth_client_contract::panels::salvage::SalvageAction::Add(item),
            )),
            // A spell bar takes spells; an item dropped there goes nowhere.
            Self::SpellSlot { .. } | Self::Window | Self::CrossSlot(_) => None,
        }
    }
}

/// Draw a window's frame: the kit's ([`dereth_ui_kit::widgets::window`]); a drop on the window
/// lands in it, not on what is under it.
pub fn window(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    state: &mut WindowState,
    title: &str,
    size: (f32, f32),
    default_pos: (f32, f32),
) -> WindowOut {
    let out = dereth_ui_kit::widgets::window(p, &mut ctx.kit, state, title, size, default_pos);
    ctx.drops.push((out.rect, None));
    out
}
