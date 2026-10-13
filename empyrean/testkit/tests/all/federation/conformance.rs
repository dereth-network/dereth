//! Divergence: V447
//! The conformance suite's world checks, with the suite playing the hub against Empyrean's network
//! half: what the world sends the hub and what its characters' clients are shown, check by check.
//! Fixture: one virtual-time world with two characters (`super::Net`), its link driven directly.

use dereth_network_conformance::world::{run, Shown, WorldUnderTest};
use dereth_network_conformance::{failures, Check};
use dereth_network_protocol::directory::{OthersSee, Seeable};
use dereth_network_protocol::wire::WireError;
use dereth_network_protocol::{DirectoryMessage, Message};
use dereth_primitives::ObjectId;
use dereth_protocol::comms::{
    CommunicationHearDirectSpeech, CommunicationModifyCharacterSquelch, CommunicationWeenieError,
    CommunicationWeenieErrorWithString,
};
use dereth_protocol::social::{SocialAddFriend, SocialFriendsUpdate};
use dereth_protocol::turbine::{decode_incoming, IncomingPayload};
use empyrean_entity::enums::{
    CharacterOption, ChatMessageType, PropertyInt, WeenieError, WeenieErrorWithString,
};
use empyrean_entity::ObjectGuid;
use empyrean_world::federation;
use empyrean_world::world_objects::{player_allegiance, player_character};

use super::{
    seed, Net, Seed, FRIENDS_UPDATE, TELL, TEXTBOX, TURBINE_CHAT, WEENIE_ERROR, WEENIE_ERROR_STR,
};

struct Empyrean {
    net: Net,
    seeds: Vec<Seed>,
    marks: std::collections::BTreeMap<String, usize>,
    /// What the world sent the hub and the checks have not yet taken, each family apart.
    sent: Vec<Message>,
    listings: Vec<DirectoryMessage>,
}

impl Empyrean {
    fn new() -> Self {
        let cat = seed("cat", 0x5000_0001, "Cat");
        let dan = seed("dan", 0x5000_0002, "Dan");
        let mut net = Net::new(&[("ww", &[cat, dan])]);
        net.enter("ww", cat);
        net.enter("ww", dan);
        let mut me = Self {
            net,
            seeds: vec![cat, dan],
            marks: std::collections::BTreeMap::new(),
            sent: Vec::new(),
            listings: Vec::new(),
        };
        for s in [cat, dan] {
            me.made(s.name);
        }
        me
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

    fn guid(&self, who: &str) -> ObjectGuid {
        let s = self
            .seeds
            .iter()
            .find(|s| s.name == who)
            .unwrap_or_else(|| panic!("no character {who}"));
        ObjectGuid::new(s.guid)
    }

    /// Sets an integer property of `who`'s character in the world.
    fn set_int(&mut self, who: &str, p: PropertyInt, value: i32) {
        let g = self.guid(who);
        if let Some(o) = self.world().objects.get_mut(g) {
            o.set_property(p, value);
        }
    }

    /// `who` as character creation leaves a player: a level, a heritage and a gender.
    fn made(&mut self, who: &str) {
        self.set_int(who, PropertyInt::Level, 1);
        self.set_int(who, PropertyInt::HeritageGroup, 1);
        self.set_int(who, PropertyInt::Gender, 1);
    }

    /// Takes what the world queued for the hub, sorted by family.
    fn drain(&mut self) {
        for f in federation::take_frames(self.world()) {
            match Message::from_json(&f) {
                Ok(m) => self.sent.push(m),
                Err(WireError::Unknown(_)) => self.listings.push(
                    DirectoryMessage::from_json(&f)
                        .expect("the world sends the directory's messages"),
                ),
                Err(e) => panic!("the world sent a malformed frame: {e}"),
            }
        }
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
        self.drain();
        std::mem::take(&mut self.sent)
    }

    fn give_directory(&mut self, m: &DirectoryMessage) {
        federation::on_link_frame(self.world(), &m.to_json());
    }

    fn take_directory(&mut self) -> Vec<DirectoryMessage> {
        self.drain();
        std::mem::take(&mut self.listings)
    }

    fn log_out(&mut self, who: &str) {
        let id = self.net.worlds[0].clients[who];
        self.net.worlds[0].ts.client_mut(id).log_off();
        self.advance(12.0);
    }

    fn log_in(&mut self, who: &str) {
        let s = *self
            .seeds
            .iter()
            .find(|s| s.name == who)
            .unwrap_or_else(|| panic!("no character {who}"));
        self.net.enter("ww", s);
        // A new client: everything it is shown is new.
        self.marks.insert(who.to_owned(), 0);
        let g = self.guid(who);
        let made = self
            .world()
            .objects
            .get(g)
            .is_some_and(|o| o.get_property(PropertyInt::Level).is_some());
        if !made {
            self.made(who);
        }
        self.advance(0.3);
    }

    fn set_level(&mut self, who: &str, level: u32) {
        self.set_int(
            who,
            PropertyInt::Level,
            i32::try_from(level).unwrap_or(i32::MAX),
        );
    }

    fn set_seeable(&mut self, who: &str, values: Seeable, allowed: OthersSee) {
        let int = |v: Option<u64>| v.and_then(|n| i32::try_from(n).ok()).unwrap_or(0);
        let count = |v: Option<u32>| v.and_then(|n| i32::try_from(n).ok()).unwrap_or(0);
        self.set_int(who, PropertyInt::CreationTimestamp, int(values.birth));
        self.set_int(who, PropertyInt::Age, int(values.age));
        self.set_int(who, PropertyInt::ChessRank, count(values.chess));
        self.set_int(who, PropertyInt::FakeFishingSkill, count(values.fishing));
        self.set_int(who, PropertyInt::NumDeaths, count(values.deaths));
        self.set_int(who, PropertyInt::NumCharacterTitles, count(values.titles));
        let g = self.guid(who);
        let w = self.world();
        for (option, on) in [
            (
                CharacterOption::AllowOthersToSeeYourDateOfBirth,
                allowed.date_of_birth,
            ),
            (CharacterOption::AllowOthersToSeeYourAge, allowed.age),
            (
                CharacterOption::AllowOthersToSeeYourChessRank,
                allowed.chess_rank,
            ),
            (
                CharacterOption::AllowOthersToSeeYourFishingSkill,
                allowed.fishing_skill,
            ),
            (
                CharacterOption::AllowOthersToSeeYourNumberOfDeaths,
                allowed.number_of_deaths,
            ),
            (
                CharacterOption::AllowOthersToSeeYourNumberOfTitles,
                allowed.number_of_titles,
            ),
        ] {
            player_character::set_character_option(w, g, option, on);
        }
    }

    fn swear(&mut self, vassal: &str, patron: &str) {
        let (v, p) = (self.guid(vassal), self.guid(patron));
        player_allegiance::swear_allegiance(self.world(), v, p.full(), true, true);
        self.advance(0.3);
    }

    fn advance(&mut self, secs: f64) {
        // Minutes pass in half seconds: the network's own times are seconds and minutes.
        let step = if secs > 5.0 { 0.5 } else { 0.05 };
        let steps = (secs / step).ceil().max(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        for _ in 0..steps as u32 {
            let w = &mut self.net.worlds[0];
            w.ts.advance(step);
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
        9,
        "the person layer's checks ran"
    );
    assert_eq!(
        checks
            .iter()
            .filter(|c| c.name.starts_with("listing."))
            .count(),
        9,
        "the directory's checks ran"
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
