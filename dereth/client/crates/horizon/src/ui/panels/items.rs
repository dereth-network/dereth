//! What an item tile does, wherever it is (a pack, the character's gear, a chest), the stack
//! splitter, and the loot window over a chest or corpse on the ground.
//!
//! A tile does what the game's item lists do: a click selects the item, a drag carries it (to
//! another slot, a pack, the character to wear it, a chest, the shortcut bar or the world), a
//! double-click uses it, and a right-click examines it. While a use is waiting for its target, a
//! click on a tile gives it that target.

use dereth_client_contract::view::DropTarget;
use dereth_client_contract::UiRequest;
use dereth_primitives::ObjectId;

use super::{split, WindowId, Windows};
use crate::art::Family;
use crate::draw::Rect;
use crate::ui::game::{GameState, Item};
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Painter, TextStyle};
use crate::ui::Outcome;

/// The gauge along an item tile's foot: its track, and its fill.
pub const STRUCTURE_TRACK: u32 = 0xFF00_0000;
pub const STRUCTURE_FILL: u32 = 0xFF7C_D8F8;

/// The wash over a tile whose thing the game is still busy with.
pub const WAITING_WASH: u32 = 0xA010_0C08;

/// Whether the game is still busy with `item`: moving, dropping, giving or using it.
#[must_use]
pub fn waiting(item: &Item) -> bool {
    item.decoration.as_ref().is_some_and(|d| d.waiting)
}

/// How much of its structure `item` has, as a share of the most it holds; `None` for a thing
/// that has none.
#[must_use]
pub fn structure(item: &Item) -> Option<f32> {
    let d = item.decoration.as_ref()?;
    #[allow(clippy::cast_precision_loss)]
    (d.max_structure > 0).then(|| (d.structure as f32 / d.max_structure as f32).clamp(0.0, 1.0))
}

/// The share the gauge along `item`'s tile shows: its structure while it is not full; `None`
/// for a full one, which shows no gauge.
#[must_use]
pub fn gauge(item: &Item) -> Option<f32> {
    let d = item.decoration.as_ref()?;
    structure(item).filter(|_| d.structure < d.max_structure)
}

impl Windows {
    /// One item tile: drawn as the game composes it, and answering the pointer as its lists do.
    pub(super) fn item_tile(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        r: Rect,
        item: Option<&Item>,
        out: &mut Outcome,
    ) {
        if let Some(it) = item {
            if ctx.input.double_clicked(&r) {
                // The first press of the pair started a drag that never moved; it goes.
                *ctx.drag = None;
                out.requests.push(if state.targeting {
                    UiRequest::ExecuteTargetItem(it.id)
                } else {
                    UiRequest::Use(it.id)
                });
                self.item_slot(p, ctx, r, item);
                return;
            }
            if ctx.input.right_clicked(&r) {
                out.requests.push(UiRequest::Select(it.id));
                out.requests.push(UiRequest::Examine(it.id));
            }
        }
        let pressed = self.item_slot(p, ctx, r, item);
        if let Some(it) = item {
            // A thing the game is still moving, dropping, giving or using is dimmed until it
            // answers, as its own tiles ghost one.
            if waiting(it) {
                p.fill(Rect::new(r.x, r.y, r.w, r.w), WAITING_WASH);
            }
            if state.target.as_ref().is_some_and(|t| t.id == it.id) {
                kit::selected_tile(p, Rect::new(r.x, r.y, r.w, r.w));
            }
            // How full a thing that fills is (a salvage bag), or how much is left of one that
            // is used up: a gauge along the tile's foot.
            if structure(it).is_some() {
                let k = p.scale;
                // A framed bar thick enough to read, but not on a full one, and what is left as
                // a number over it.
                if let Some(full) = gauge(it) {
                    let bar = Rect::new(r.x + 3.0 * k, r.y + r.w - 9.0 * k, r.w - 6.0 * k, 6.0 * k);
                    p.fill(bar.inset(-k), STRUCTURE_TRACK);
                    p.fill(bar, 0xFF30_3030);
                    p.fill(Rect::new(bar.x, bar.y, bar.w * full, bar.h), STRUCTURE_FILL);
                }
                if let Some(d) = it.decoration.as_ref() {
                    let style = TextStyle::new(crate::art::Family::Body, 11.0, 0xFFFF_FFFF)
                        .edge(0xFF00_0000);
                    p.text(
                        &style,
                        r.x + 3.0 * k,
                        r.y + 1.0 * k,
                        &d.structure.to_string(),
                    );
                }
            }
        }
        if pressed {
            if let Some(it) = item {
                if state.targeting {
                    out.requests.push(UiRequest::ExecuteTargetItem(it.id));
                } else {
                    *ctx.drag = Some(crate::ui::Drag {
                        item: it.id,
                        look: Some(it.clone()),
                        spell_look: None,
                        from_shortcut: None,
                        icon: it.ac_icon,
                        origin: ctx.input.mouse,
                        active: false,
                        on_click: Some(UiRequest::Select(it.id)),
                        spell: None,
                        from_spell_slot: None,
                        component: false,
                    });
                }
            }
        }
    }

    /// What a drop on slot `index` of `container`'s grid asks for: the item placed there, in
    /// front of whatever is in that slot now, as the game's item lists place a drop.
    pub(super) fn slot_drop(
        ctx: &Ctx<'_>,
        state: &GameState,
        container: ObjectId,
        items: &[Item],
        index: usize,
    ) -> DropTarget {
        let dragged = ctx.drag.as_ref().map(|d| d.item);
        let dragged_is_container = dragged.is_some_and(|id| {
            state.side_packs.iter().any(|(pack, _)| pack.id == id)
                || state
                    .loot
                    .as_ref()
                    .is_some_and(|(_, _, l)| l.iter().any(|i| i.id == id && i.container))
        });
        let index = index.min(items.len());
        DropTarget::ItemListSlot {
            container,
            under: items.get(index).map(|i| i.id),
            index: u32::try_from(index).unwrap_or(0),
            num_ui_items: u32::try_from(items.len()).unwrap_or(0),
            dragged_is_container,
            container_list: false,
        }
    }

    /// The split follows the selection, as the game's toolbar seeds it: a newly selected stack
    /// (or one whose size changed) is split whole, and anything else is one. A stack is split
    /// wherever it is: in a pack, worn, in a chest or corpse, or on the ground.
    pub(super) fn follow_split(&mut self, state: &GameState, out: &mut Outcome) {
        let stack = state.target.as_ref().and_then(|t| {
            let n = t.look.as_ref().and_then(|l| l.stack).or_else(|| {
                state
                    .pack
                    .iter()
                    .chain(state.side_packs.iter().flat_map(|(_, items)| items.iter()))
                    .chain(state.equipped.iter())
                    .chain(state.loot.iter().flat_map(|(_, _, items)| items.iter()))
                    .find(|i| i.id == t.id)
                    .and_then(|i| i.stack)
            })?;
            (n > 1).then_some((t.id, n))
        });
        let (next, request) = split::follow(stack, self.split);
        if next != self.split {
            self.split = next;
            if self.typing_field == split::FIELD {
                self.typing_field = 0;
            }
        }
        out.requests.extend(request);
    }

    /// Put the keyboard in the stack splitter's number, while there is a stack to split: what is
    /// typed next is the number, in place of the one shown.
    pub fn focus_splitter(&mut self) {
        if self.split.is_some() {
            self.typing_field = split::FIELD;
            self.split_typed.clear();
        }
    }

    /// The stack splitter, while a stack is selected: how many a drag of it moves, from one to
    /// the whole stack, set on the slider, typed, or stepped with the wheel over either.
    pub(super) fn split_row(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        r: Rect,
        out: &mut Outcome,
    ) {
        let Some((id, amount, max)) = self.split else {
            return;
        };
        let k = p.scale;
        let label = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let value = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        p.text(&label, r.x, r.y + 5.0 * k, "Split");
        let all = Rect::new(r.right() - 44.0 * k, r.y, 44.0 * k, r.h);
        let of = format!("/ {max}");
        let of_w = p.measure(&value, &of);
        let typed = Rect::new(all.x - 18.0 * k - of_w - 64.0 * k, r.y, 64.0 * k, r.h);
        let track = Rect::new(
            r.x + 44.0 * k,
            r.y,
            (typed.x - 12.0 * k - (r.x + 44.0 * k)).max(20.0 * k),
            r.h,
        );
        let mut next = amount;
        // The slider: one at the left, the whole stack at the right.
        #[allow(clippy::cast_precision_loss)]
        let t = if max > 1 {
            (amount - 1) as f32 / (max - 1) as f32
        } else {
            1.0
        };
        if let Some(t) = kit::track_slider(p, ctx, track, t) {
            next = split::at(t, max);
        }
        // The box shows the split, but while it is being typed in.
        if self.typing_field != split::FIELD {
            self.split_typed = amount.to_string();
        }
        // Enter takes the count and lets the keyboard go, for the keys of the world.
        if kit::text_box(
            p,
            ctx,
            typed,
            &mut self.split_typed,
            &mut self.typing_field,
            split::FIELD,
        ) {
            self.typing_field = 0;
        }
        if self.typing_field == split::FIELD {
            self.split_typed.retain(|c| c.is_ascii_digit());
            if let Some(n) = split::typed(&self.split_typed, max) {
                next = n;
            }
        }
        p.text(&value, typed.right() + 6.0 * k, r.y + 4.0 * k, &of);
        if kit::button(p, ctx, all, "All", amount < max) {
            next = max;
        }
        // The wheel over the slider or the box steps by one, or ten with Shift.
        if (ctx.input.hover(&track) || ctx.input.hover(&typed)) && ctx.input.wheel != 0.0 {
            let step = if ctx.input.shift { 10 } else { 1 };
            next = if ctx.input.wheel > 0.0 {
                amount.saturating_add(step).min(max)
            } else {
                amount.saturating_sub(step).max(1)
            };
            ctx.input.wheel = 0.0;
        }
        if next != amount {
            self.split = Some((id, next, max));
            out.requests
                .push(UiRequest::StackSliderChanged { split: next, max });
        }
    }

    /// Whether the stack being split is one of `items`.
    pub(super) fn splitting_one_of(&self, items: &[Item]) -> bool {
        self.split
            .is_some_and(|(id, _, _)| items.iter().any(|i| i.id == id))
    }

    /// The loot window follows the chest or corpse the game has open on the ground: it opens
    /// with one, closes when the game closes it, and closing it closes the chest.
    pub(super) fn follow_loot(&mut self, ctx: &mut Ctx<'_>, state: &GameState, out: &mut Outcome) {
        let ground = state.loot.as_ref().map(|(id, _, _)| *id);
        match (ground, self.loot_open) {
            (Some(id), open) if open != Some(id) => {
                self.open(WindowId::Loot, ctx.time);
                self.loot_open = Some(id);
            }
            (None, Some(_)) => {
                self.close(WindowId::Loot);
                self.loot_open = None;
            }
            (Some(id), Some(_)) if !self.is_open(WindowId::Loot) => {
                out.requests.push(UiRequest::CloseExternalContainer(id));
                self.loot_open = None;
            }
            _ => {}
        }
    }

    /// The chest's contents as a grid: take an item by dragging it to a pack (or double-click
    /// to use it), put one in by dropping it here.
    pub(super) fn loot(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let Some((id, name, items)) = &state.loot else {
            return;
        };
        let title = TextStyle::new(Family::Body, 15.0, 0xFFFF_FFFF).edge(ctx.colours.edge());
        p.text(&title, body.x + 8.0 * k, body.y + 4.0 * k, name);
        let slot = 44.0 * k;
        let gap = 6.0 * k;
        // As many columns as fit the body with room for the frames round the tiles, up to eight.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cols = (((body.w - 8.0 * k + gap) / (slot + gap)).floor() as usize).clamp(1, 8);
        #[allow(clippy::cast_precision_loss)]
        let grid_w = cols as f32 * (slot + gap) - gap;
        // Centred, and never out past the body's left edge (the frames stand a little outside
        // their tiles).
        let x0 = body.x + ((body.w - grid_w) / 2.0).max(4.0 * k);
        let y0 = body.y + 36.0 * k;
        // The splitter at the foot while the stack being split is in here.
        let splitting = self.splitting_one_of(items);
        let foot = if splitting { 60.0 } else { 26.0 };
        let area = Rect::new(body.x, y0, body.w, body.bottom() - y0 - foot * k);
        ctx.drops
            .push((area, Some(DropTarget::Container(*id).into())));
        let rows = items.len().max(1).div_ceil(cols).max(4);
        // The clip leaves the tiles' frames, which stand a little outside them, whole.
        let margin = 4.0 * k;
        p.list.push_clip(Rect::new(
            area.x - margin,
            area.y - margin,
            area.w + 2.0 * margin,
            area.h + 2.0 * margin,
        ));
        for i in 0..rows * cols {
            let (c, r) = (i % cols, i / cols);
            #[allow(clippy::cast_precision_loss)]
            let rect = Rect::new(
                x0 + c as f32 * (slot + gap),
                y0 + r as f32 * (slot + gap),
                slot,
                slot + 2.0 * k,
            );
            if rect.y > area.bottom() {
                break;
            }
            ctx.drops.push((
                rect,
                Some(Self::slot_drop(ctx, state, *id, items, i).into()),
            ));
            self.item_tile(p, ctx, state, rect, items.get(i), out);
        }
        p.list.pop_clip();
        if splitting {
            self.split_row(
                p,
                ctx,
                Rect::new(
                    body.x + 8.0 * k,
                    body.bottom() - 54.0 * k,
                    body.w - 16.0 * k,
                    24.0 * k,
                ),
                out,
            );
        }
        let small = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let n = items.len();
        p.text(
            &small,
            body.x + 8.0 * k,
            body.bottom() - 20.0 * k,
            &format!("{n} item{}", if n == 1 { "" } else { "s" }),
        );
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;
    use dereth_client_contract::view::SlotDecoration;

    fn with_structure(structure: u32, max_structure: u32) -> Item {
        Item {
            decoration: Some(SlotDecoration {
                structure,
                max_structure,
                ..SlotDecoration::default()
            }),
            ..Item::default()
        }
    }

    #[test]
    fn an_item_tile_shows_a_gauge_only_while_it_is_not_full() {
        assert_eq!(gauge(&with_structure(25, 100)), Some(0.25));
        assert_eq!(gauge(&with_structure(0, 100)), Some(0.0), "empty");
        assert_eq!(gauge(&with_structure(100, 100)), None, "full");
        assert_eq!(gauge(&with_structure(120, 100)), None, "over full");
        assert_eq!(gauge(&with_structure(5, 0)), None, "nothing to fill");
        assert_eq!(gauge(&Item::default()), None);
        assert_eq!(
            structure(&with_structure(100, 100)),
            Some(1.0),
            "a full one still has its count"
        );
    }
}
