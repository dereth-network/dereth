use super::*;
use dereth_client_contract::feedback::{Feedback, FeedbackKind};
use dereth_client_model::{NoticeSink, Weenie};
use dereth_client_net::client_session::SessionEvent;
use dereth_protocol::{Message, Opcode};

#[derive(Debug, Default)]
struct Receiver(Vec<(u8, String, Feedback)>);
impl crate::hud::HudPanels for Receiver {
    fn spew_offer(&mut self, ty: u8, body: &str, feedback: Feedback) -> bool {
        self.0.push((ty, body.into(), feedback));
        ty == 0x1a
    }
    fn spew_trace(&self) -> (bool, usize, u64) {
        (true, self.0.len(), 0)
    }
    fn spew_clear_pending(&mut self) {
        self.0.clear();
    }
    fn abuse_response(&mut self, _: u32) {}
}
fn event<M: Message>(m: &M) -> SessionEvent {
    let mut blob = M::OPCODE.0.to_le_bytes().to_vec();
    blob.extend(dereth_protocol::write_body(m).unwrap());
    SessionEvent::UiEvent {
        opcode: M::OPCODE,
        blob,
    }
}
fn deliver(
    hud: &mut crate::hud::Hud,
    world: &mut dereth_client_model::World,
    panels: &mut Receiver,
    events: &[SessionEvent],
) -> Vec<dereth_client_contract::chat::interface::ChatMessage> {
    hud.apply_events_with_combat_mode_handler(events, world, panels, None, &mut |_, _| {})
}

/// Behaviour: feedback.delivery.producer-metadata-survives-local-and-network-delivery
#[test]
fn real_use_and_decoded_messages_keep_meaning_through_notice_absorb_and_both_hud_routes() {
    let mut world = dereth_client_model::World::new();
    world.player = Some(ObjectId(3));
    let mut source = Weenie::new(ObjectId(1));
    source.pwd.name = "Unfamiliar Gem".into();
    source.pwd.useability = Some(0x0020_0020);
    source.pwd.target_type = Some(1);
    source.pwd.bitfield = dereth_rules::weenie::bitfield::STUCK;
    let mut target = Weenie::new(ObjectId(2));
    target.pwd.name = "Unfamiliar Ring".into();
    target.pwd.obj_type = 1;
    world.tables.weenies.insert(source.id, source);
    world.tables.weenies.insert(target.id, target);
    world.targeting_object = ObjectId(1);
    let mut notices = Notices::default();
    let mut requests = RecordingRequests::default();
    world.use_object(
        &mut requests,
        &mut notices,
        ObjectId(1),
        Default::default(),
        ServerTime(1.0),
    );
    let mut interaction = Interaction::new();
    interaction.absorb(&mut world, notices, requests);
    let mut hud = crate::hud::Hud::new();
    let mut receiver = Receiver::default();
    let local = deliver(&mut hud, &mut world, &mut receiver, &[]);
    assert_eq!(local.len(), 1);
    assert_eq!(local[0].body, "Choose a target for the Unfamiliar Gem");
    assert_eq!(local[0].feedback, Feedback::INFORMATION);
    assert_eq!(local[0].window, 0);
    assert_eq!(world.scroll.added, 1);
    assert_eq!(interaction.stats.notice_strings_scrolled, 1);
    assert_eq!(
        receiver.0,
        vec![(0x1a, local[0].body.clone(), Feedback::INFORMATION)]
    );
    assert!(deliver(&mut hud, &mut world, &mut receiver, &[]).is_empty());
    assert_eq!(receiver.0.len(), 1);

    let text = local[0].body.clone();
    let transient = dereth_protocol::comms::CommunicationTransientString { text: text.clone() };
    let ordinary = dereth_protocol::comms::CommunicationTextboxString {
        text: text.clone(),
        text_type: 0x1a,
    };
    let decoded = deliver(
        &mut hud,
        &mut world,
        &mut receiver,
        &[event(&transient), event(&ordinary)],
    );
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].feedback, Feedback::SERVER_TRANSIENT);
    assert_eq!(decoded[1].feedback, Feedback::ORDINARY);
    assert_eq!(decoded[0].body, decoded[1].body);
    assert_eq!(receiver.0.len(), 3);
    assert_eq!(receiver.0[1].2.kind, FeedbackKind::ServerTransient);
    assert_eq!(receiver.0[2].2.kind, FeedbackKind::Ordinary);
    // The second transient opcode uses the same decoded operation.
    let mut alternate = event(&transient);
    if let SessionEvent::UiEvent { opcode, blob } = &mut alternate {
        *opcode = Opcode::COMMUNICATION_TRANSIENT_STRING_0317;
        blob[..4].copy_from_slice(&opcode.0.to_le_bytes());
    }
    assert_eq!(
        deliver(&mut hud, &mut world, &mut receiver, &[alternate])[0].feedback,
        Feedback::SERVER_TRANSIENT
    );
}

/// Behaviour: feedback.delivery.producer-metadata-survives-local-and-network-delivery
#[test]
fn identical_local_and_ordinary_lines_do_not_exchange_metadata_or_duplicate_logging() {
    use std::sync::{Arc, Mutex};
    #[derive(Debug)]
    struct Log(Arc<Mutex<Vec<String>>>);
    impl dereth_primitives::TextSink for Log {
        fn write_line(&mut self, line: &str) {
            self.0.lock().unwrap().push(line.into());
        }
    }
    let mut world = dereth_client_model::World::new();
    let lines = Arc::new(Mutex::new(Vec::new()));
    assert!(world
        .scroll
        .start_copy_output_to_file("memory", 0, || Some(Box::new(Log(lines.clone())))));
    let mut notices = Notices::default();
    notices.emit(Notice::DisplayString {
        channel: 0,
        text: "identical".into(),
        feedback: Feedback::INFORMATION,
    });
    notices.emit(Notice::DisplayString {
        channel: 0,
        text: "identical".into(),
        feedback: Feedback::ORDINARY,
    });
    notices.emit(Notice::DisplayString {
        channel: 0,
        text: "identical".into(),
        feedback: Feedback::WARNING,
    });
    Interaction::new().absorb(&mut world, notices, RecordingRequests::default());
    let mut hud = crate::hud::Hud::new();
    let mut receiver = Receiver::default();
    let got = deliver(
        &mut hud,
        &mut world,
        &mut receiver,
        &[event(&dereth_protocol::comms::CommunicationTextboxString {
            text: "network".into(),
            text_type: 0,
        })],
    );
    assert_eq!(
        got.iter().map(|m| m.feedback).collect::<Vec<_>>(),
        [
            Feedback::INFORMATION,
            Feedback::ORDINARY,
            Feedback::WARNING,
            Feedback::ORDINARY
        ]
    );
    assert_eq!(receiver.0.len(), 4);
    assert_eq!(
        lines.lock().unwrap().as_slice(),
        ["identical", "identical", "identical", "network"]
    );
    assert!(deliver(&mut hud, &mut world, &mut receiver, &[]).is_empty());
    assert_eq!(receiver.0.len(), 4);
    assert_eq!(lines.lock().unwrap().len(), 4);
}
