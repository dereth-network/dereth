//! Shipped layout properties and resolver installed in an isolated UI system.

use dereth_primitives::{AssetSource, DataId};
use dereth_ui::framework::DidMapperResolver;
use dereth_ui::{UiFlow, UiSystem};
use std::rc::Rc;

pub(crate) enum RegistrationOrder {
    BeforeResolver,
    AfterResolver,
}

pub(crate) fn load(
    display: (i32, i32),
    order: RegistrationOrder,
) -> (UiSystem, UiFlow, Rc<dereth_dat::RetailDatStore>) {
    let store = dereth_dat::testing::open_store_or_fail();
    let id = DataId(0x3900_0001);
    let master = <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(
        id,
        &store.read(id).expect("master property payload"),
    )
    .expect("master property table");
    let mut ui = UiSystem::new(display);
    ui.property_types = master.property_types();
    let mut flow = UiFlow::new();
    if matches!(order, RegistrationOrder::BeforeResolver) {
        dereth_ui_screens::register_all(&mut ui, &mut flow);
    }
    let store = Rc::new(store);
    let resolver =
        Rc::new(DidMapperResolver::load_via_master(store.as_ref()).expect("layout resolver"));
    dereth_ui_screens::env::install(&mut ui, store.clone(), resolver);
    if matches!(order, RegistrationOrder::AfterResolver) {
        dereth_ui_screens::register_all(&mut ui, &mut flow);
    }
    (ui, flow, store)
}

#[derive(Debug)]
pub(crate) struct Strings(pub(crate) Rc<dereth_dat::RetailDatStore>);

impl dereth_ui::text::StringResolver for Strings {
    fn resolve_raw(&self, table: DataId, string_id: u32) -> Option<String> {
        self.resolve_variants_raw(table, string_id)?
            .into_iter()
            .next()
    }
    fn resolve_variants_raw(&self, table: DataId, string_id: u32) -> Option<Vec<String>> {
        use dereth_assets::Decode;
        let b = self.0.read(table).ok()?;
        let t = dereth_assets::ui::StringTable::decode_payload(table, &b).ok()?;
        t.strings
            .into_iter()
            .find(|(k, _)| *k == string_id)
            .map(|(_, v)| v.strings)
    }
}
