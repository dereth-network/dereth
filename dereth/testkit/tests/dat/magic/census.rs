use dereth_primitives::ObjectId;
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound};
use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::panels::remaining::SPELL_PAGE;
use dereth_ui_screens::panels::spellcomponent::{self, COMPONENT_LIST};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

use super::{examine, strip};

/// A whole client with the recorded session's server half replayed into it, the spell page up
/// and the Components tab pressed.
pub fn a_recorded_pack() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.world_mut().player = Some(super::A_RECORDED_PLAYER);
    c.when(Inbound::from_corpus(
        super::A_RECORDED_SESSION,
        0..usize::MAX,
    ));
    c.tick(3);
    open_the_components_tab(&mut c);
    c
}

fn open_the_components_tab(c: &mut HeadlessClient) {
    examine::open_the_spellbook(c);
    c.tick(3);
    let tab = components_tab(c);
    click(c, tab, 1_000);
    c.tick(3);
    let list = list(c);
    assert!(
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell")
            .ui
            .is_visible(list),
        "pressing the Components tab brings the list up"
    );
}

/// The Components tab's caption, discovered off the spell page's own tab table -- the
/// sub-page whose subtree carries the components list.
fn components_tab(c: &HeadlessClient) -> ElemHandle {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the UI shell").ui;
    let root = screen(c).root().expect("the gameplay root");
    let page = ui
        .get_child_recursive(root, SPELL_PAGE)
        .expect("the spell page");
    let pairs: Vec<(ElementId, ElementId)> = ui
        .node(page)
        .and_then(|n| {
            n.behaviour
                .as_ref()?
                .as_any()?
                .downcast_ref::<dereth_ui::widgets::panel::Panel>()
        })
        .expect("the spell page is a tabbed panel")
        .page_to_tab
        .iter()
        .map(|(p, t)| (*p, *t))
        .collect();
    assert!(pairs.len() >= 2, "the spell page is tabbed");
    for (page_id, tab_id) in pairs {
        let Some(pe) = ui.get_child_recursive(page, page_id) else {
            continue;
        };
        if ui.get_child_recursive(pe, COMPONENT_LIST).is_some() {
            return ui
                .get_child_recursive(page, tab_id)
                .expect("that sub-page's tab caption");
        }
    }
    panic!("no sub-page of the spell page carries the components list");
}

fn screen(c: &HeadlessClient) -> &GamePlayScreen {
    let any: &dyn std::any::Any = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell")
        .flow
        .current()
        .expect("a screen");
    any.downcast_ref().expect("the gameplay screen")
}

pub fn list(c: &HeadlessClient) -> ElemHandle {
    let root = screen(c).root().expect("the gameplay root");
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell")
        .ui
        .get_child_recursive(root, COMPONENT_LIST)
        .expect("the components list")
}

/// The list's items, in order: headings and rows interleaved.
pub fn items(c: &HeadlessClient) -> Vec<ElemHandle> {
    let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
    ui.node(list(c))
        .and_then(|n| {
            n.behaviour
                .as_ref()?
                .as_any()?
                .downcast_ref::<dereth_ui::widgets::listbox::ListBox>()
        })
        .expect("the components list is a list box")
        .items
        .clone()
}

pub fn item_ids(c: &mut HeadlessClient) -> Vec<ElementId> {
    items(c).iter().map(|h| element_id(c, *h)).collect()
}

/// Which item the list has selected.
pub fn selected_index(c: &HeadlessClient) -> Option<usize> {
    let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
    ui.node(list(c))
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| (**b).as_any())
        .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
        .and_then(|l| l.selected)
}

pub fn element_id(c: &HeadlessClient, h: ElemHandle) -> ElementId {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell")
        .ui
        .node(h)
        .expect("alive")
        .element_id()
}

/// The drawn rows, with the premise that the recording filled the pack stated rather than
/// assumed.
pub fn drawn(c: &HeadlessClient) -> Vec<spellcomponent::DrawnRow> {
    let p = strip::panel(c);
    assert!(p.bound(), "the shipped tree carries the components list");
    assert_eq!(
        p.templates(),
        2,
        "with its heading template and its row template"
    );
    assert!(p.rebuilds >= 1, "and the page has been built at least once");
    assert!(
        !p.rows.is_empty(),
        "the recorded session's pack carries spell components; {} objects are tracked",
        c.view().world().magic.components.tracked_objects()
    );
    p.rows.clone()
}

/// The tracker's own census: `(kind order, kind, name, held)` in the page's walk order.
pub fn tracker_rows(c: &HeadlessClient) -> Vec<(u32, u32, String, i64)> {
    c.view()
        .world()
        .magic
        .components
        .categories()
        .flat_map(|(k, rows)| {
            rows.iter()
                .map(|d| (k, d.class_id, d.name.clone(), d.num_items()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The kinds the tracker has rows in, in its own order.
pub fn kinds_the_tracker_has(c: &HeadlessClient) -> Vec<u32> {
    c.view()
        .world()
        .magic
        .components
        .categories()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(k, _)| k)
        .collect()
}

/// One real press on a list item, with the hit test asserted first so that a hit-test miss
/// cannot masquerade as a missing selection. For a list row the hit is the **list box**: the
/// list owns the press and finds the row under the pointer itself.
pub fn click(c: &mut HeadlessClient, h: ElemHandle, at: u32) {
    let (x, y) = {
        let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
        let mut parent = Some(h);
        while let Some(p) = parent {
            assert!(
                ui.node(p).expect("alive").region.flags.visible,
                "a visible ancestor"
            );
            parent = ui.parent(p);
        }
        let r = ui.screen_clip_box(h);
        assert!(r.is_valid(), "the item is on screen to be pressed");
        let at = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        let hit = ui.hit_test_screen(at.0, at.1);
        assert!(
            hit.is_some_and(|hit| hit == h || ui.is_ancestor_of(h, hit) || owning_list(ui, hit, h)),
            "the hit test picks that item, a descendant of it, or the list box over it"
        );
        at
    };
    examine::press(c, x, y, at, examine::PRIMARY);
}

fn owning_list(ui: &dereth_ui::UiSystem, hit: ElemHandle, h: ElemHandle) -> bool {
    ui.node(hit).map(dereth_ui::ElementNode::element_id) == Some(COMPONENT_LIST)
        && ui.is_ancestor_of(hit, h)
}

/// The first heading past the first that is on screen to be pressed.
pub fn a_header_past_the_first(c: &HeadlessClient) -> Option<(usize, ElemHandle)> {
    let items = items(c);
    let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
    items.iter().enumerate().skip(1).find_map(|(i, h)| {
        if element_id(c, *h) != spellcomponent::HEADER_ELEMENT {
            return None;
        }
        let r = ui.screen_clip_box(*h);
        let at = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        let hit = ui.hit_test_screen(at.0, at.1);
        (r.is_valid()
            && hit.is_some_and(|x| x == *h || ui.is_ancestor_of(*h, x) || owning_list(ui, x, *h)))
        .then_some((i, *h))
    })
}

/// Whether the client asked the shard to *do* anything since `from`. A selection asks for no
/// action at all.
pub fn asked_for_an_action(c: &HeadlessClient, from: usize) -> bool {
    c.outbound()[from..].iter().any(|r| {
        matches!(
            r,
            dereth_client_model::Request::UseEvent(_)
                | dereth_client_model::Request::UseWithTargetEvent(_)
        )
    })
}

/// A second pile of `source`'s kind, made by replaying the recording's **own** create for it
/// with the object id changed. Nothing is fabricated but the id: the body is the recorded one
/// and it goes through the same reader the recording's other blobs do, so the pile arrives in
/// the same container with the same kind, name and stack.
pub fn a_second_pile_of(c: &mut HeadlessClient, source: ObjectId, fresh: ObjectId) {
    /// The create the shard sends for one object.
    const CREATE: u32 = 0xF745;
    let corpus =
        dereth_client_net::client_session::testing::Corpus::load(super::A_RECORDED_SESSION)
            .expect("the recording parses")
            .expect("the decoded corpus carries it");
    let rows: Vec<&dereth_client_net::client_session::testing::CorpusBlob> = corpus
        .blobs
        .iter()
        .filter(|r| {
            r.dir == dereth_client_net::client_session::testing::Direction::ServerToClient
                && r.opcode == CREATE
                && r.payload.get(4..8) == Some(source.0.to_le_bytes().as_slice())
        })
        .collect();
    assert_eq!(
        rows.len(),
        1,
        "exactly one recorded create makes {source:?}"
    );
    let mut payload = rows[0].payload.clone();
    payload[4..8].copy_from_slice(&fresh.0.to_le_bytes());
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode(CREATE),
            body: payload[4..].to_vec(),
        },
    ));
    c.tick(2);
    let w = c.view().world();
    assert!(
        w.weenie(fresh).is_some(),
        "the replayed create made the second pile"
    );
    assert_eq!(
        w.magic.components.object_is_owned_component(fresh),
        w.magic.components.object_is_owned_component(source),
        "and it is tracked as the same kind"
    );
}
