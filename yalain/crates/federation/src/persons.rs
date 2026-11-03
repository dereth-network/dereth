//! Persons, the network's link between a player's characters and their Discord account
//! (`docs/network/03-world-behaviour.md` section 9, and section 1's *A person's Discord*).
//!
//! The hub keeps persons; the world asks on its own characters' behalf and shows what the hub
//! answers.
//!
//! - **Linking.** `@dn link` asks the hub for a one-time code, and shows the link it gives;
//!   `@dn unlink` undoes it. A link holds only for the character that asked for its code
//!   ([`ASKED_RECORD`]): a character made under the name of a deleted one is not linked to the
//!   deleted one's person, and is unlinked, quietly.
//! - **The view.** While a character on the network is present the world watches its person view,
//!   which says whether it is linked. The friends the view's persons list are not listed here: a
//!   character's friends stay its own, kept on the world.
//! - **Reach.** When the hub asks whether a character, away, would have been shown a tell, the world
//!   answers true only for a character of its own, away, on the network, linked by itself, ignoring
//!   neither the teller nor its world and blocking neither, within the pace of that world's tells
//!   to it. A tell of this world's own never goes through the hub, so a teller of this world is
//!   answered false.

use std::collections::BTreeSet;

use dereth_network_protocol::answers::person_line;
use dereth_network_protocol::text::{sanitize_line, sanitize_name};
use dereth_network_protocol::{Address, Message, PersonOp, PersonOutcome, ReachMessage};
use yalain_store::Store;

use crate::{Ask, AskKind, Federation, Friends, LinkState, NOT_ON, TOO_QUICKLY, UNAVAILABLE};

/// The world's record of the characters that asked for a link, one id a line.
pub const ASKED_RECORD: &str = "dereth-network-asked-links";

/// What a character is told when the view first says it is linked, while it is here.
pub const NOW_LINKED: &str = "This character is now linked to a Discord account.";

/// What a linked character going off the network is told: its link pauses, and `@dn on` resumes it
/// with nothing to redo.
pub const PAUSED: &str =
    "This character's link to your Discord account is paused until you say @dn on.";

/// The characters a record says asked for a link.
pub(crate) fn read_asked(record: Option<&[u8]>) -> BTreeSet<u64> {
    record
        .map(String::from_utf8_lossy)
        .map(|r| r.lines().filter_map(|l| l.trim().parse().ok()).collect())
        .unwrap_or_default()
}

impl Federation {
    /// Whether a character is linked, as the hub last said. Not known while it is away.
    #[must_use]
    pub fn is_linked(&self, name: &str) -> bool {
        self.linked
            .get(&name.to_lowercase())
            .copied()
            .unwrap_or(false)
    }

    /// Whether a character asked for its link (by id).
    #[must_use]
    pub fn asked_link(&self, character: u64) -> bool {
        self.asked_link.contains(&character)
    }

    /// `@dn link` for a character at `t`: the line it is told at once, when it is refused here. The
    /// link the hub gives is shown as it answers.
    pub fn link_person(
        &mut self,
        store: &mut impl Store,
        t: f64,
        character: u64,
        name: &str,
    ) -> Option<String> {
        if let Err(why) = self.may_ask_person(t, name) {
            return Some(why.to_owned());
        }
        if self.asked_link.insert(character) {
            let record: String = self.asked_link.iter().map(|c| format!("{c}\n")).collect();
            if let Err(e) = store.set_world_record(ASKED_RECORD, record.as_bytes()) {
                tracing::warn!("Dereth Network: the store: {e}");
                self.asked_link.remove(&character);
                return Some("The Dereth Network can't keep that now. Try again later.".to_owned());
            }
        }
        self.ask_person(t, character, name, PersonOp::Code, false);
        None
    }

    /// `@dn unlink` for a character at `t`: the line it is told at once, when it is refused here.
    pub fn unlink_person(&mut self, t: f64, character: u64, name: &str) -> Option<String> {
        if let Err(why) = self.may_ask_person(t, name) {
            return Some(why.to_owned());
        }
        self.ask_person(t, character, name, PersonOp::Unlink, false);
        None
    }

    /// A player's person request: the character on the network, within its pace of requests.
    fn may_ask_person(&mut self, t: f64, name: &str) -> Result<(), &'static str> {
        if !self.is_on(name) {
            return Err(NOT_ON);
        }
        if !self.may_ask(t, name) {
            return Err(TOO_QUICKLY);
        }
        Ok(())
    }

    /// Takes one from a character's pace of requests of the hub, kept beside its tells' at the
    /// same rate.
    pub(crate) fn may_ask(&mut self, t: f64, name: &str) -> bool {
        let rate = self.limits.tells_per_character;
        self.sends
            .entry(format!("{}\n@asks", name.to_lowercase()))
            .or_insert_with(|| dereth_network_protocol::limits::Bucket::new(rate, t))
            .take(t)
    }

    fn ask_person(&mut self, t: f64, character: u64, name: &str, op: PersonOp, quiet: bool) {
        let Some(who) = self.on_name(name).and_then(|n| self.own_address(n)) else {
            return;
        };
        let id = self.fresh_id('p');
        self.asks.insert(
            id.clone(),
            Ask {
                character,
                kind: AskKind::Person { op, quiet },
                sent: None,
            },
        );
        self.send(
            t,
            Message::Person {
                id,
                op,
                who,
                target: None,
                targets: Vec::new(),
                shown: None,
            },
        );
    }

    /// Asks the hub for a character's person view, or stops: only while the link is up (a welcome
    /// asks for everyone's again). Stopped, what the view said is forgotten.
    pub(crate) fn watch_person(&mut self, name: &str, on: bool) {
        if !on {
            self.linked.remove(&name.to_lowercase());
        }
        if self.link != LinkState::Up {
            return;
        }
        if let Some(who) = self.on_name(name).and_then(|n| self.own_address(n)) {
            self.person_watches
                .push_back(Message::PersonWatch { who, on });
        }
    }

    /// Every present character on the network asks for its view again (after a welcome).
    pub(crate) fn watch_every_person(&mut self, friends: &impl Friends) {
        for name in friends.everyone_present() {
            if self.is_on(&name) {
                self.watch_person(&name, true);
            }
        }
    }

    /// The answer to a person request.
    pub(crate) fn on_person_result(
        &mut self,
        friends: &mut impl Friends,
        id: &str,
        outcome: PersonOutcome,
        code: Option<&str>,
        url: Option<&str>,
        expires_secs: Option<u32>,
    ) {
        let Some(a) = self.asks.remove(id) else {
            return;
        };
        let AskKind::Person { op, quiet: false } = a.kind else {
            return;
        };
        if let Some(line) = person_line(op, outcome, "", code, url, expires_secs) {
            friends.show_line(a.character, &sanitize_line(&line));
        }
    }

    /// A person view from the hub for a character of this world in the world. A link only holds for
    /// the character that asked for it: one that never asked is taken as unlinked, and the hub told
    /// so, quietly.
    pub(crate) fn on_person_view(
        &mut self,
        friends: &mut impl Friends,
        t: f64,
        to: &Address,
        linked: bool,
    ) {
        if to.world() != self.settings.slug || !friends.in_world(to.name()) {
            return;
        }
        let Some(on) = self.on.get(&to.name().to_lowercase()).cloned() else {
            return;
        };
        let asked = self.asked_link.contains(&on.id);
        if linked && !asked && self.may_ask(t, &on.name) {
            self.ask_person(t, on.id, &on.name, PersonOp::Unlink, true);
        }
        let linked = linked && asked;
        let before = self.linked.insert(on.name.to_lowercase(), linked);
        if linked && before == Some(false) {
            friends.show_line(on.id, NOW_LINKED);
        }
    }

    /// A person or channel request that will not be answered.
    pub(crate) fn give_up_ask(&mut self, friends: &mut impl Friends, id: &str) {
        if let Some(a) = self.asks.remove(id) {
            if !matches!(a.kind, AskKind::Person { quiet: true, .. }) {
                friends.show_line(a.character, UNAVAILABLE);
            }
        }
    }

    /// One of the reach messages from the hub: a question is answered; an answer, which only a world
    /// sends, is ignored. An answer waits with those owed to the teller's world.
    pub(crate) fn on_reach(&mut self, friends: &impl Friends, t: f64, m: ReachMessage) {
        let ReachMessage::Ask { id, from, to } = m else {
            return;
        };
        if self.link != LinkState::Up {
            return;
        }
        let origin = from.world().to_owned();
        if !self.may_owe(&origin) {
            tracing::debug!(
                "Dereth Network: {origin} is owed its share of answers; a question is not taken"
            );
            return;
        }
        let reachable = self.reachable(friends, t, &from, &to);
        self.owe_frame(t, &origin, ReachMessage::Result { id, reachable }.to_json());
    }

    /// Whether `to`, one of this world's characters away, would have been shown a tell from `from`;
    /// a `true` takes one from the pace of tells from `from`'s world to `to`.
    fn reachable(&mut self, friends: &impl Friends, t: f64, from: &Address, to: &Address) -> bool {
        if to.world() != self.settings.slug || from.world() == self.settings.slug {
            return false;
        }
        let Some(on) = self.on.get(&to.name().to_lowercase()) else {
            return false;
        };
        let shown = self.asked_link.contains(&on.id)
            && !friends.in_world(to.name())
            && !self.blocked(from)
            && !friends.ignores(to.name(), from)
            && sanitize_name(&from.to_string()).is_ok();
        let rate = self.limits.tells_per_recipient;
        shown && self.may_receive(t, from.world(), &to.key(), rate)
    }
}
