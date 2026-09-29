//! The icon-composition bench, shared by the dat tier's `inventory::icon_composite` and the gpu
//! tier's (which declares this file by path): every recording replayed into a GPU-less gameplay
//! screen built from the shipped layout, at the instant its world holds the most objects.
//!
//! Behaviour: none (shared fixtures)

#![allow(dead_code)]

use std::rc::Rc;

use dereth_client::hud::Hud;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client_net::client_session::testing::{session_names, shared_session};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_primitives::{AssetSource, DataId, LocalTime, ObjectId};
use dereth_ui::framework::Screen;
use dereth_ui::region::IconRecipe;
use dereth_ui::UiSystem;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

// =================================================================================================
// The capture reader.
// =================================================================================================

/// Every recording the corpus index names, in name order.
pub(crate) fn corpus_sessions() -> Vec<String> {
    let mut out: Vec<String> = session_names().iter().map(|s| (*s).to_owned()).collect();
    out.sort();
    out
}

pub(crate) fn replay_to(session: &str, limit: usize) -> (ObjectStream, Vec<SessionEvent>) {
    let records = shared_session(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).expect("the capture has no LoginRequest"),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    let mut events = Vec::new();
    for r in records.iter().take(limit) {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
            events.push(e);
        }
    }
    (objects, events)
}

/// Return the replay limit just after the first maximum object population. The end may be
/// emptied by logoff, so sample the populated instant without assuming every recording logs off.
pub(crate) fn busiest_instant(session: &str) -> usize {
    let records = shared_session(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).expect("the capture has no LoginRequest"),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    let mut best = (0usize, 1usize);
    for (i, r) in records.iter().enumerate() {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
        }
        let n = objects.world.tables.weenies.len();
        if n > best.0 {
            best = (n, i + 1);
        }
    }
    best.1
}

pub(crate) fn scene(session: &str) -> (ObjectStream, Hud) {
    let (mut objects, events) = replay_to(session, busiest_instant(session));
    let mut hud = Hud::new();
    // Loading tables is required: Hud::build_spells returns an empty book without its spell
    // table. Omitting this producer would make later spell checks vacuous; their nonempty-book
    // denominator must accompany the data join.
    hud.load_tables(&open_store(), &objects.world);
    let _ = hud.apply_events(&events, &mut objects.world);
    hud.sync(&objects, None);
    (objects, hud)
}

// =================================================================================================
// Gameplay elements constructed from the shipped layout without a GPU.
// =================================================================================================

#[derive(Debug)]
struct Store(dereth_dat::RetailDatStore);

impl AssetSource for Store {
    fn read(&self, id: DataId) -> Result<Vec<u8>, dereth_primitives::AssetError> {
        self.0.read(id)
    }
    fn exists(&self, id: DataId) -> bool {
        self.0.exists(id)
    }
    fn iter_type(
        &self,
        kind: dereth_primitives::DataType,
    ) -> Box<dyn Iterator<Item = DataId> + '_> {
        self.0.iter_type(kind)
    }
}

pub(crate) fn open_store() -> dereth_dat::RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

pub(crate) fn shipped_gameplay() -> (UiSystem, Box<dyn Screen>) {
    use dereth_ui::framework::DidMapperResolver;
    let store = open_store();
    let master_id = DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("MasterProperty 0x39000001");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("decode MasterProperty");
    let mut ui = UiSystem::new((800, 600));
    ui.property_types = master.property_types();
    let mut flow = dereth_ui::UiFlow::new();
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    let store = Rc::new(Store(store));
    let resolver =
        Rc::new(DidMapperResolver::load_via_master(store.as_ref()).expect("the DidMapper"));
    dereth_ui_screens::env::install(&mut ui, store, resolver);
    let mut screen = GamePlayScreen::create_screen();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen is created from its real layout");
    (ui, screen)
}

pub(crate) fn as_gameplay(screen: &mut Box<dyn Screen>) -> &mut GamePlayScreen {
    let any: &mut dyn std::any::Any = &mut **screen;
    any.downcast_mut::<GamePlayScreen>()
        .expect("gameplay screen")
}

pub(crate) struct Bench {
    pub(crate) ui: UiSystem,
    pub(crate) screen: Box<dyn Screen>,
    pub(crate) objects: ObjectStream,
    pub(crate) hud: Hud,
}

impl Bench {
    pub(crate) fn open(session: &str) -> Self {
        let (mut ui, screen) = shipped_gameplay();
        let (objects, hud) = scene(session);
        ui.requests.clear();
        let mut b = Self {
            ui,
            screen,
            objects,
            hud,
        };
        b.hud
            .drive(&mut b.ui, as_gameplay(&mut b.screen), 1, &b.objects);
        b
    }

    /// Every live slot of every one of the inventory page's lists, as `(item, recipe)`.
    pub(crate) fn cells(&mut self) -> Vec<(Option<ObjectId>, Option<IconRecipe>)> {
        let g = as_gameplay(&mut self.screen);
        let p = &g.inventory;
        let mut out = Vec::new();
        let push = |w: &dereth_ui_screens::items::widget::ItemListWidget,
                    ui: &UiSystem,
                    out: &mut Vec<(Option<ObjectId>, Option<IconRecipe>)>| {
            for s in &w.slots {
                out.push((s.item, s.icon_recipe(ui)));
            }
        };
        for w in p
            .top_container
            .iter()
            .chain(p.container_list.iter())
            .chain(p.item_list.iter())
        {
            push(w, &self.ui, &mut out);
        }
        for (_, w) in &p.doll {
            push(w, &self.ui, &mut out);
        }
        out
    }
}
