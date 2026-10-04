use dereth_primitives::ObjectId;
use dereth_testkit::HeadlessClient;

pub fn ringed(c: &mut HeadlessClient, item: ObjectId) -> Vec<bool> {
    let found: Vec<Option<dereth_ui::ElemHandle>> = {
        let (_ui, screen) = super::gameplay_screen(c.app_mut());
        let bar: Vec<Option<dereth_ui::ElemHandle>> = screen
            .shortcuts
            .slots
            .iter()
            .flat_map(|w| w.slots.iter())
            .filter(|s| s.item == Some(item))
            .map(|s| s.selected_ring)
            .collect();
        screen
            .inventory
            .lists_mut()
            .flat_map(|w| w.slots.iter())
            .filter(|s| s.item == Some(item))
            .map(|s| s.selected_ring)
            .chain(bar)
            .collect()
    };
    let (ui, _root) = super::gameplay_root(c.app_mut());
    found
        .into_iter()
        .map(|h| h.is_some_and(|h| ui.node(h).is_some_and(|n| n.region.flags.visible)))
        .collect()
}
