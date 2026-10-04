//! Spell filtering, drawable favorites and spell-bar decisions shared by every interface.
use crate::{GameView, UiRequest};
use dereth_primitives::ObjectId;

/// The default spell-filter setting when section `0x0020` is absent — every
/// school and every level. The protocol decoder uses the same
/// number; it is repeated here because this crate does not depend on `dereth-protocol`.
pub const DEFAULT_SPELL_FILTERS: u32 = 0x3FFF;

/// The **first** formula slot, decrypted — subtracts the key
/// from every non-zero slot in place, so slot 0 keeps its position and a zero slot stays zero.
///
/// `dereth_assets::tables::SpellBase::comps` drops the zero slots, which loses that position; the
/// power component has to be read off `raw_comps[0]` and the key.
#[must_use]
pub const fn power_component(raw_comp0: u32, comp_key: u32) -> u32 {
    if raw_comp0 == 0 {
        0
    } else {
        raw_comp0.wrapping_sub(comp_key)
    }
}

/// The school and level masks use different, disjoint bits.
#[must_use]
pub const fn school_mask(school: u32) -> u32 {
    match school {
        1 => 8,
        2 => 4,
        3 => 2,
        4 => 1,
        5 => 0x2000,
        _ => 0,
    }
}
/// The ninth level has a persisted bit even when no control exposes it.
#[must_use]
pub const fn level_mask(level: u32) -> u32 {
    if level >= 1 && level <= 9 {
        0x10 << (level - 1)
    } else {
        0
    }
}
#[must_use]
pub const fn accepts(filters: u32, school: u32, level: u32) -> bool {
    filters & school_mask(school) != 0 && filters & level_mask(level) != 0
}
/// Convert the persisted school/level mask to compact sets in Creature, Item, Life, War, Void order.
#[must_use]
pub const fn filter_sets(mask: u32) -> (u8, u16) {
    let mut schools = 0;
    let values = [4, 3, 2, 1, 5];
    let mut i = 0;
    while i < values.len() {
        if mask & school_mask(values[i]) != 0 {
            schools |= 1 << i;
        }
        i += 1;
    }
    let mut levels = 0;
    let mut level = 1;
    while level <= 9 {
        if mask & level_mask(level) != 0 {
            levels |= 1 << (level - 1);
        }
        level += 1;
    }
    (schools, levels)
}
/// Encode the compact school and level sets into the persisted filter mask.
#[must_use]
pub const fn filter_mask(schools: u8, levels: u16) -> u32 {
    let values = [4, 3, 2, 1, 5];
    let mut mask = 0;
    let mut i = 0;
    while i < values.len() {
        if schools & (1 << i) != 0 {
            mask |= school_mask(values[i]);
        }
        i += 1;
    }
    let mut level = 1;
    while level <= 9 {
        if levels & (1 << (level - 1)) != 0 {
            mask |= level_mask(level);
        }
        level += 1;
    }
    mask
}
/// Missing metadata does not consume a drawable row or a quickslot.
#[must_use]
pub fn drawable_favorites(view: &dyn GameView, tab: usize) -> Vec<u32> {
    view.spell_tab(tab)
        .iter()
        .copied()
        .filter(|id| view.spellbook().iter().any(|s| s.id == *id))
        .collect()
}
/// A move removes only a displayed row. Duplicate refusal inspects the stored bank instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FavoritePlan {
    pub remove: bool,
    pub index: i32,
}
impl FavoritePlan {
    #[must_use]
    pub fn new(
        raw: &[u32],
        rows: &[u32],
        spell: u32,
        mut index: i32,
        move_allowed: bool,
    ) -> Option<Self> {
        if spell == 0 || (!move_allowed && raw.contains(&spell)) {
            return None;
        }
        let old = rows.iter().position(|id| *id == spell);
        if old.is_some_and(|i| i32::try_from(i).unwrap_or(i32::MAX) < index) {
            index -= 1;
        }
        if index == -1 {
            index = i32::try_from(rows.len() + 1 - usize::from(old.is_some())).unwrap_or(i32::MAX);
        }
        Some(Self {
            remove: old.is_some(),
            index,
        })
    }
    #[must_use]
    pub fn requests(self, spell_id: u32, tab: usize) -> Vec<UiRequest> {
        let mut requests = Vec::new();
        if self.remove {
            requests.push(UiRequest::RemoveSpellFavorite { spell_id, tab });
        }
        requests.push(UiRequest::AddSpellFavorite {
            spell_id,
            index: self.index,
            tab,
        });
        requests
    }
}
/// Spell and endowed-item selection remain independent: caption and cast have different precedence.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SpellSelection {
    pub spell: u32,
    pub endowment: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMove {
    Previous,
    Next,
    First,
    Last,
}
impl SpellSelection {
    /// An explicit cast refuses once; callers guard empty quickslots before reaching this.
    #[must_use]
    pub fn cast(self, item: Option<ObjectId>) -> UiRequest {
        if let Some(item) = item.filter(|_| self.endowment) {
            return UiRequest::Use(item);
        }
        if self.spell != 0 {
            UiRequest::CastSpell {
                spell_id: self.spell,
            }
        } else {
            UiRequest::DisplayChatText {
                feedback: crate::feedback::Feedback::LOCAL,
                channel: 0x1a,
                text: "You must select a spell to cast".into(),
            }
        }
    }
    #[must_use]
    pub fn moved(self, rows: &[u32], endowed: bool, movement: SelectionMove) -> Option<Self> {
        let endowment = Self {
            spell: 0,
            endowment: true,
        };
        let at = |i| Self {
            spell: rows[i],
            endowment: false,
        };
        if rows.is_empty() {
            return endowed.then_some(endowment);
        }
        let last = rows.len() - 1;
        match movement {
            SelectionMove::First if endowed => Some(endowment),
            SelectionMove::First => Some(at(0)),
            SelectionMove::Last => Some(at(last)),
            SelectionMove::Next | SelectionMove::Previous => {
                let next = movement == SelectionMove::Next;
                if self.endowment {
                    return Some(at(if next { 0 } else { last }));
                }
                let Some(i) = rows.iter().position(|id| *id == self.spell) else {
                    return Some(at(0));
                };
                if (next && i == last) || (!next && i == 0) {
                    Some(if endowed {
                        endowment
                    } else {
                        at(if next { 0 } else { last })
                    })
                } else {
                    Some(at(if next { i + 1 } else { i - 1 }))
                }
            }
        }
    }
}
/// A quiet readiness result; adapters decide wording and line breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellReadiness {
    Ready,
    ReadyOnTarget,
    NeedsTarget,
    Incompatible,
    Missing,
}
#[must_use]
pub fn readiness(view: &dyn GameView, spell: u32, item: Option<ObjectId>) -> SpellReadiness {
    let self_target = if spell != 0 {
        let Some(entry) = view.spell(spell) else {
            return SpellReadiness::Missing;
        };
        view.spell_is_untargeted(spell) || entry.bitfield & 8 != 0
    } else if let Some(item) = item {
        if view.name(item).is_none() {
            return SpellReadiness::Missing;
        }
        view.item_useable_self_target(item)
    } else {
        return SpellReadiness::Missing;
    };
    if self_target {
        return SpellReadiness::Ready;
    }
    if view.selected_object().is_none_or(|id| id.0 == 0) {
        return SpellReadiness::NeedsTarget;
    }
    let compatible = if spell != 0 {
        view.spell_target_compatible(spell)
    } else {
        item.is_some_and(|item| view.item_target_compatible(item))
    };
    if compatible {
        SpellReadiness::ReadyOnTarget
    } else {
        SpellReadiness::Incompatible
    }
}
