//! Divergence: V447
//! The conformance suite's world checks, with the suite playing the hub against Empyrean's network
//! half: what the world sends the hub and what its characters' clients are shown, check by check.
//! Fixture: one virtual-time world with two characters (`super::Net`), its link driven directly.

use dereth_network_conformance::world::{run, Shown, WorldUnderTest};
use dereth_network_conformance::{failures, Check};
use dereth_network_protocol::Message;
use dereth_primitives::ObjectId;
use dereth_protocol::comms::{
    CommunicationHearDirectSpeech, CommunicationModifyCharacterSquelch, CommunicationWeenieError,
    CommunicationWeenieErrorWithString,
};
use dereth_protocol::social::{SocialAddFriend, SocialFriendsUpdate};
use dereth_protocol::turbine::{decode_incoming, IncomingPayload};
use empyrean_entity::enums::{ChatMessageType, WeenieError, WeenieErrorWithString};
use empyrean_world::federation;

use super::{
    seed, Net, FRIENDS_UPDATE, TELL, TEXTBOX, TURBINE_CHAT, WEENIE_ERROR, WEENIE_ERROR_STR,
};

struct Empyrean {
    net: Net,
    marks: std::collections::BTreeMap<String, usize>,
}

impl Empyrean {
    fn new() -> Self {
        let cat = seed("cat", 0x5000_0001, "Cat");
        let dan = seed("dan", 0x5000_0002, "Dan");
        let mut net = Net::new(&[("ww", &[cat, dan])]);
        net.enter("ww", cat);
        net.enter("ww", dan);
        Self {
            net,
            marks: std::collections::BTreeMap::new(),
        }
    }

    fn world(&mut self) -> &mut empyrean_world::World {
        &mut self.net.worlds[0].ts.world
    }

    fn action<M: dereth_protocol::Message>(&mut self, who: &str, m: &M) {
        let w = &mut self.net.worlds[0];
        let id = w.clients[who];
        w.ts.send_game_action(id, m);
        self.advance(0.3);
    }
}

impl WorldUnderTest for Empyrean {
    fn slug(&self) -> String {
        "ww".into()
    }

    fn characters(&self) -> (String, String) {
        ("Cat".into(), "Dan".into())
    }

    fn link_up(&mut self) {
        federation::on_link_up(self.world());
    }

    fn link_down(&mut self) {
        federation::on_link_down(self.world());
    }

    fn give(&mut self, m: &Message) {
        federation::on_link_frame(self.world(), &m.to_json());
    }

    fn take(&mut self) -> Vec<Message> {
        federation::take_frames(self.world())
            .iter()
            .map(|f| Message::from_json(f).expect("the world sends messages"))
            .collect()
    }

    fn advance(&mut self, secs: f64) {
        let steps = (secs / 0.05).ceil().max(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        for _ in 0..steps as u32 {
            let w = &mut self.net.worlds[0];
            w.ts.advance(0.05);
            federation::tick(&mut w.ts.world);
        }
    }

    fn command(&mut self, who: &str, line: &str) {
        let w = &mut self.net.worlds[0];
        let id = w.clients[who];
        w.ts.send_game_action(
            id,
            &dereth_protocol::comms::CommunicationTalk {
                message: line.to_owned(),
            },
        );
        self.advance(0.2);
    }

    fn tell(&mut self, who: &str, to: &str, text: &str) {
        self.action(
            who,
            &dereth_protocol::comms::CommunicationTalkDirectByName {
                message: text.to_owned(),
                target_name: to.to_owned(),
            },
        );
    }

    fn tell_id(&mut self, who: &str, id: u32, text: &str) {
        self.action(
            who,
            &dereth_protocol::comms::CommunicationTalkDirect {
                message: text.to_owned(),
                target: ObjectId(id),
            },
        );
    }

    fn add_friend(&mut self, who: &str, name: &str) {
        self.action(
            who,
            &SocialAddFriend {
                name: name.to_owned(),
            },
        );
    }

    fn squelch(&mut self, who: &str, name: &str) {
        self.action(
            who,
            &CommunicationModifyCharacterSquelch {
                add: 1,
                character_id: ObjectId(0),
                character_name: name.to_owned(),
                msg_type: ChatMessageType::AllChannels.0,
            },
        );
    }

    fn shown(&mut self, who: &str) -> Vec<Shown> {
        let mark = self.marks.get(who).copied().unwrap_or(0);
        let now = self.net.mark("ww", who);
        self.marks.insert(who.to_owned(), now);
        let mut out = Vec::new();
        for g in self.net.since("ww", who, mark) {
            match g.kind {
                TELL => {
                    let t: CommunicationHearDirectSpeech = g.decode();
                    out.push(Shown::Tell {
                        from: t.sender_name,
                        id: t.sender_id.0,
                        text: t.message,
                    });
                }
                TEXTBOX => out.push(Shown::Line(
                    g.plain::<dereth_protocol::comms::CommunicationTextboxString>()
                        .text,
                )),
                WEENIE_ERROR => {
                    if g.decode::<CommunicationWeenieError>().error_type
                        == WeenieError::CharacterNotAvailable.0.cast_unsigned()
                    {
                        out.push(Shown::NotAvailable);
                    }
                }
                WEENIE_ERROR_STR => {
                    let e: CommunicationWeenieErrorWithString = g.decode();
                    if e.error_type == WeenieErrorWithString::AFK.0.cast_unsigned() {
                        out.push(Shown::Away(e.text));
                    }
                }
                FRIENDS_UPDATE => {
                    let u: SocialFriendsUpdate = g.decode();
                    for f in u.friends {
                        out.push(Shown::FriendRow {
                            kind: u.update_type,
                            id: f.id.0,
                            name: f.name,
                            online: f.online != 0,
                        });
                    }
                }
                TURBINE_CHAT => {
                    if let Ok(packet) = decode_incoming(&g.blob[4..]) {
                        if let IncomingPayload::RoomEvent(e) = packet.payload {
                            let id = e.extra.get(0..4).map_or(0, |b| {
                                u32::from_le_bytes(b.try_into().expect("four bytes"))
                            });
                            out.push(Shown::RoomLine {
                                name: e.name,
                                text: e.text,
                                id,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }
}

/// Divergence: V447
#[test]
fn empyrean_passes_every_world_check() {
    let checks: Vec<Check> = run(&mut || Box::new(Empyrean::new()));
    let failed = failures(&checks);
    assert!(checks.len() >= 24, "{} checks ran", checks.len());
    assert_eq!(
        checks
            .iter()
            .filter(|c| c.name.starts_with("person."))
            .count(),
        10,
        "the person layer's checks ran"
    );
    assert!(
        failed.is_empty(),
        "{}",
        failed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}
