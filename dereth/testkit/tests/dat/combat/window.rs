use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::ObjectId;
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound};
use dereth_ui::{Delivery, ElementId, Screen as _, UiSystem};
use dereth_ui_screens::hud::combat_notice as cn;
use dereth_ui_screens::screens::gameplay::{window as win, GamePlayScreen};

/// The shipped gameplay screen over a loaded scene, which is what the combat cluster is drawn
/// into.
pub fn a_client_in_the_world() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay_in_world(4))
}

/// The character the shard's own updates are about. The weenie has to exist, because that is
/// the object the character's numbers live on.
pub fn this_character_is(c: &mut HeadlessClient, id: ObjectId) {
    let w = c.world_mut();
    w.player = None;
    w.tables
        .weenies
        .insert(id, dereth_client_model::weenie::Weenie::new(id));
    assert!(w.set_player(id), "the identity is adopted once");
    c.hud_mut().player_desc_received = true;
}

/// One frame, plus the bounded delivery pass the queued show-and-hide messages need.
pub fn settle(c: &mut HeadlessClient) {
    c.tick(1);
    for _ in 0..8 {
        let (ui, screen) = parts(c);
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                screen.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
}

pub fn parts(c: &mut HeadlessClient) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = c.app_mut().ui_mut().expect("the UI shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a current screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen");
    (ui, screen)
}

fn visible(c: &mut HeadlessClient, id: ElementId) -> bool {
    let (ui, screen) = parts(c);
    let root = screen.root().expect("the gameplay root");
    let h = ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped tree"));
    ui.node(h).expect("alive").region.flags.visible
}

/// `(the cluster of controls, the window itself)`.
pub fn the_window_is_up(c: &mut HeadlessClient) -> (bool, bool) {
    (
        visible(c, cn::COMBAT_UI_PAGE),
        visible(c, win::COMBAT_PANEL),
    )
}

/// `(the fighting cluster, the casting page)`.
pub fn cluster_pages(c: &mut HeadlessClient) -> (bool, bool) {
    (
        visible(c, cn::COMBAT_UI_PAGE),
        visible(c, cn::SPELLCASTING_PAGE),
    )
}

/// The modes whose toolbar button is lit.
pub fn lit_mode_buttons(c: &mut HeadlessClient) -> Vec<u32> {
    let buttons: Vec<(u32, ElementId)> = dereth_ui_screens::toolbar::combat_mode::BUTTONS.to_vec();
    buttons
        .into_iter()
        .filter(|(_, id)| visible(c, *id))
        .map(|(m, _)| m)
        .collect()
}

/// Which set of combat keys the client is carrying.
pub fn combat_keys(c: &mut HeadlessClient) -> u32 {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .combat_input_mode()
}

/// The shard's own mode change through the model, for the scenario whose subject is the
/// window rather than the message.
pub fn the_shard_sets_the_mode(c: &mut HeadlessClient, m: dereth_client_model::combat::CombatMode) {
    c.world_mut()
        .set_combat_mode(
            &mut dereth_client_model::NullRequests,
            &mut dereth_client_model::RecordingSink::default(),
            m,
            false,
            true,
            false,
        )
        .expect("the shard's own form takes no ready check");
}

/// One quality update, built the way the shard writes it: the private form when no subject is
/// named, and the public one when one is.
pub fn the_shard_says(
    subject: Option<ObjectId>,
    sequence: u8,
    property: u32,
    value: u32,
) -> Inbound {
    Inbound::event(update(subject, sequence, property, value))
}

/// The same message with its last byte missing, which is what an undecodable one is.
pub fn a_truncated_update(sequence: u8, property: u32, value: u32) -> Inbound {
    let mut e = update(None, sequence, property, value);
    if let SessionEvent::UiEvent { blob, .. } = &mut e {
        blob.pop();
    }
    Inbound::event(e)
}

fn update(subject: Option<ObjectId>, sequence: u8, property: u32, value: u32) -> SessionEvent {
    let opcode = if subject.is_some() {
        dereth_protocol::Opcode::QUALITIES_UPDATE_INT
    } else {
        dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_INT
    };
    let mut blob = opcode.0.to_le_bytes().to_vec();
    blob.push(sequence);
    if let Some(id) = subject {
        blob.extend_from_slice(&id.0.to_le_bytes());
    }
    blob.extend_from_slice(&property.to_le_bytes());
    blob.extend_from_slice(&value.to_le_bytes());
    SessionEvent::UiEvent { opcode, blob }
}
