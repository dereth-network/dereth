//! One salvage session, independent of the interface displaying it.

use crate::World;
use dereth_client_contract::panels::salvage::{SalvageAction, SalvageListView, SalvageNotice};
use dereth_primitives::ObjectId;

#[derive(Debug, Clone, Default)]
pub struct SalvageList {
    state: SalvageListView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SalvageEffect {
    Notice(String),
    Submit {
        tool: ObjectId,
        items: Vec<ObjectId>,
    },
}

impl SalvageList {
    pub fn view(&self) -> SalvageListView {
        self.state.clone()
    }

    pub fn open(&mut self, tool: ObjectId) {
        self.state = SalvageListView {
            tool: (tool.0 != 0).then_some(tool),
            visible: true,
            ..Default::default()
        };
    }

    pub fn clear(&mut self) {
        self.state.items.clear();
        self.state.material = 0;
    }

    pub fn close(&mut self) {
        self.clear();
        self.state.tool = None;
        self.state.visible = false;
    }

    pub fn add(&mut self, id: ObjectId, material: u32, suitable: bool, multiple: bool) -> bool {
        if !suitable
            || self.state.items.contains(&id)
            || (!multiple && self.state.material != 0 && self.state.material != material)
        {
            return false;
        }
        if self.state.items.is_empty() {
            self.state.material = material;
        }
        self.state.items.push(id);
        true
    }

    pub fn remove(&mut self, id: ObjectId) {
        self.state.items.retain(|i| *i != id);
        if self.state.items.is_empty() {
            self.state.material = 0;
        }
    }

    pub fn submit(&mut self) -> Option<SalvageEffect> {
        let tool = self.state.tool.filter(|_| !self.state.items.is_empty())?;
        let items = self.state.items.iter().rev().copied().collect();
        self.clear();
        Some(SalvageEffect::Submit { tool, items })
    }
}

impl World {
    pub fn salvage_list_view(&self) -> SalvageListView {
        self.salvage.view()
    }

    pub fn salvage_action(&mut self, action: SalvageAction, multiple: bool) -> Vec<SalvageEffect> {
        let mut list = std::mem::take(&mut self.salvage);
        let mut effects = Vec::new();
        match action {
            SalvageAction::Add(id) => {
                if !self.is_owned_by_player(id) {
                    if self.weenie(id).is_some() {
                        effects.push(SalvageEffect::Notice(
                            "You can only salvage items that you own!".into(),
                        ));
                    }
                } else {
                    self.offer_salvage_tree(&mut list, id, multiple, &mut Vec::new(), &mut effects);
                }
            }
            SalvageAction::Remove(id) => list.remove(id),
            SalvageAction::Clear => list.clear(),
            SalvageAction::Close => list.close(),
            SalvageAction::Submit => effects.extend(list.submit()),
        }
        self.salvage = list;
        effects
    }

    pub fn salvage_notice(&mut self, notice: SalvageNotice, multiple: bool) -> Vec<SalvageEffect> {
        match notice {
            SalvageNotice::Open(tool) => {
                self.salvage.open(tool);
                Vec::new()
            }
            SalvageNotice::Add(id) => self.salvage_action(SalvageAction::Add(id), multiple),
            SalvageNotice::Remove(id) => {
                if self.salvage.state.tool == Some(id) {
                    self.salvage.close();
                } else {
                    self.salvage.remove(id);
                }
                Vec::new()
            }
        }
    }

    fn offer_salvage_tree(
        &self,
        list: &mut SalvageList,
        id: ObjectId,
        multiple: bool,
        seen: &mut Vec<ObjectId>,
        effects: &mut Vec<SalvageEffect>,
    ) {
        if seen.contains(&id) {
            return;
        }
        seen.push(id);
        let Some(w) = self.weenie(id) else { return };
        let children = self.inventory(id).map_or(&[][..], |i| i.items.as_slice());
        if !children.is_empty() {
            effects.push(SalvageEffect::Notice(
                w.object_name(crate::weenie::NameType::Appropriate),
            ));
            for &child in children {
                self.offer_salvage_tree(list, child, multiple, seen, effects);
            }
        } else {
            list.add(
                id,
                w.pwd.material_type.unwrap_or(0),
                super::is_item_suitable(w, true, 0),
                multiple,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: salvage.window.only-a-suitable-thing-goes-in-and-the-first-one-fixes-the-material
    #[test]
    fn nested_container_notice_adds_unique_leaves_in_order_and_tool_loss_closes() {
        let mut world = World::new();
        world.player = Some(ObjectId(1));
        for (id, parent, name, material) in [
            (2, 1, "Outer", 0),
            (3, 2, "Inner", 0),
            (7, 2, "First", 16),
            (6, 3, "Second", 16),
            (5, 3, "Other", 17),
        ] {
            let mut item = crate::Weenie::new(ObjectId(id));
            item.valid = true;
            item.pwd.name = name.into();
            item.pwd.container_id = Some(ObjectId(parent));
            item.pwd.material_type = Some(material);
            item.pwd.structure = Some(40);
            world.tables.weenies.insert(item.id, item);
        }
        for (id, items) in [
            (2, vec![ObjectId(7), ObjectId(3)]),
            (3, vec![ObjectId(6), ObjectId(7), ObjectId(5)]),
        ] {
            world.tables.inventories.insert(
                ObjectId(id),
                crate::objects::ObjectInventory {
                    items,
                    ..Default::default()
                },
            );
        }
        world.salvage_notice(SalvageNotice::Open(ObjectId(9)), false);
        assert_eq!(
            world.salvage_notice(SalvageNotice::Add(ObjectId(2)), false),
            vec![
                SalvageEffect::Notice("Outer".into()),
                SalvageEffect::Notice("Inner".into())
            ]
        );
        assert_eq!(
            world.salvage_list_view().items,
            vec![ObjectId(7), ObjectId(6)]
        );
        assert_eq!(world.salvage_list_view().material, 16);
        assert_eq!(
            world.salvage_action(SalvageAction::Submit, false),
            vec![SalvageEffect::Submit {
                tool: ObjectId(9),
                items: vec![ObjectId(6), ObjectId(7)]
            }]
        );
        world.salvage_notice(SalvageNotice::Add(ObjectId(7)), false);
        world.salvage_notice(SalvageNotice::Remove(ObjectId(9)), false);
        assert_eq!(world.salvage_list_view(), SalvageListView::default());
    }

    /// Behaviour: salvage.window.only-a-suitable-thing-goes-in-and-the-first-one-fixes-the-material
    #[test]
    fn removing_first_of_two_materials_retains_the_original_latch() {
        let mut list = SalvageList::default();
        list.open(ObjectId(9));
        assert!(list.add(ObjectId(7), 60, true, true));
        assert!(list.add(ObjectId(2), 61, true, true));
        list.remove(ObjectId(7));
        assert_eq!(list.view().material, 60);
        assert!(!list.add(ObjectId(6), 61, true, false));
        assert!(list.add(ObjectId(5), 60, true, false));
        assert!(!list.add(ObjectId(5), 60, true, false));
        assert_eq!(list.view().items, vec![ObjectId(2), ObjectId(5)]);
    }

    /// Behaviour: salvage.button.the-button-sends-the-tool-and-every-row-and-empties-the-window
    #[test]
    fn submission_reverses_rows_once_and_keeps_the_tool() {
        let mut list = SalvageList::default();
        list.open(ObjectId(9));
        for id in [7, 2, 6] {
            assert!(list.add(ObjectId(id), 60, true, false));
        }
        assert_eq!(
            list.submit(),
            Some(SalvageEffect::Submit {
                tool: ObjectId(9),
                items: vec![ObjectId(6), ObjectId(2), ObjectId(7)]
            })
        );
        assert!(list.view().items.is_empty());
        assert_eq!(list.view().material, 0);
        assert_eq!(list.view().tool, Some(ObjectId(9)));
        assert!(list.submit().is_none());
    }

    /// Behaviour: salvage.window.closing-it-empties-it-and-forgets-the-tool
    #[test]
    fn close_resets_contents_material_and_tool() {
        let mut list = SalvageList::default();
        list.open(ObjectId(9));
        list.add(ObjectId(7), 60, true, false);
        list.close();
        assert_eq!(list.view(), SalvageListView::default());
        list.add(ObjectId(8), 16, true, false);
        list.open(ObjectId(10));
        assert!(list.view().items.is_empty());
        assert_eq!(list.view().material, 0);
        assert_eq!(list.view().tool, Some(ObjectId(10)));
    }
}
