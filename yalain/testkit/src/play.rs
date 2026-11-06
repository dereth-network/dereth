//! Playing a client through the steps every test of the world takes: logging in to the character
//! list, creating a character, entering the world, sending in the world, and reading what came
//! back.

use auberean_primitives::Vec3;
use auberean_protocol::login::{CharacterCreated, CreateCharacter, DataHeld, PlayCharacter};
use auberean_protocol::physics::ClientPosition;
use auberean_protocol::script::{Arg, FunctionId, ScriptArgs, ServerCall};
use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::text::{NarrowString, WideString};
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_store::Store;
use yalain_world::session::function;

use crate::{Harness, Received};

/// How many steps a test waits for an answer.
pub const STEPS: usize = 400;

impl<S: Store> Harness<S> {
    /// A new client logged in with this account, its data query answered; returns once the
    /// character list is in.
    pub fn at_character_list(&mut self, account: &str) -> usize {
        let b = self.add_bot();
        let now = self.now;
        self.bots[b].log_in(account, b"ticket", now);
        assert!(self.run_until(STEPS, |h| h.bots[b]
            .messages(false)
            .iter()
            .any(|m| matches!(m, ServerMessage::DataQuery(_)))));
        let held = ClientMessage::DataHeld(DataHeld {
            language: 0x8000_0001,
            files: Vec::new(),
        });
        self.bots[b].send(false, queue::DATABASE, &held);
        assert!(self.run_until(STEPS, |h| h.bots[b]
            .messages(false)
            .iter()
            .any(|m| matches!(m, ServerMessage::CharacterList(_)))));
        b
    }

    /// A client logged in, its data not yet told: the character list waits for that.
    pub fn logging_in(&mut self, account: &str) -> usize {
        let b = self.add_bot();
        let now = self.now;
        self.bots[b].log_in(account, b"ticket", now);
        assert!(self.run_until(STEPS, |h| h.bots[b]
            .messages(false)
            .iter()
            .any(|m| matches!(m, ServerMessage::DataQuery(_)))));
        b
    }

    /// Create a human man with this name; returns its id.
    pub fn create_character(&mut self, b: usize, account: &str, name: &str) -> u64 {
        let before = self.bots[b].inbox.len();
        let m = ClientMessage::CreateCharacter(CreateCharacter {
            account: NarrowString::from(account),
            name: WideString::from(name),
            entity_did: 0x4700_110F,
            start_area: 0x4000_0001,
            race: 1,
            sex: 0x1000,
            class: 2,
            implement: 1,
            physique: Table::from_inserted(SMALL, [(1, 1.0), (2, 1.0)]),
        });
        self.bots[b].send(false, queue::LOGON, &m);
        assert!(self.run_until(STEPS, |h| h.bots[b].inbox.len() > before));
        match self.bots[b].inbox[before].message() {
            Some(ServerMessage::CharacterCreated(CharacterCreated { identity, .. })) => identity.id,
            other => panic!("not a creation answer: {other:?}"),
        }
    }

    /// Enter the world with a character, by the referral; returns once the welcome has arrived
    /// on the world connection and a few more steps have run.
    pub fn enter_world(&mut self, b: usize, account: &str, id: u64) {
        let before = self.bots[b].inbox.len();
        let m = ClientMessage::PlayCharacter(PlayCharacter {
            character: id,
            account: NarrowString::from(account),
        });
        self.bots[b].send(false, queue::LOGON, &m);
        let welcome = |r: &Received| {
            r.world
                && matches!(r.message(), Some(ServerMessage::PlayerCall(c)) if c.function.0 == function::SHOW_TEXT)
        };
        assert!(
            self.run_until(STEPS, |h| h.bots[b].inbox[before..].iter().any(welcome)),
            "the entry arrives"
        );
        self.run(10);
    }

    /// A new client logged in with a new human man in the world; returns the client and the
    /// character's id.
    pub fn player_in_world(&mut self, account: &str, name: &str) -> (usize, u64) {
        let b = self.at_character_list(account);
        let id = self.create_character(b, account, name);
        self.enter_world(b, account, id);
        (b, id)
    }

    /// The messages a client received on its world connection since `from`, read, in arrival
    /// order.
    #[must_use]
    pub fn world_messages(&self, b: usize, from: usize) -> Vec<ServerMessage> {
        self.bots[b].inbox[from..]
            .iter()
            .filter(|r| r.world)
            .filter_map(Received::message)
            .collect()
    }

    /// Step until a client has received a world message `pick` accepts since `from`; returns it.
    pub fn wait_for<T>(
        &mut self,
        b: usize,
        from: usize,
        mut pick: impl FnMut(&ServerMessage) -> Option<T>,
    ) -> Option<T> {
        for _ in 0..STEPS {
            if let Some(t) = self.world_messages(b, from).iter().find_map(&mut pick) {
                return Some(t);
            }
            self.step();
        }
        None
    }
}

/// A use of an object as the client asks for it: a use description naming the object and the
/// player.
#[must_use]
pub fn use_of(item: u64, player: u64) -> Arg {
    use yalain_world::graph::{Builder, Slot};
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    Arg::Object(b.graph())
}

impl<S: Store> Harness<S> {
    /// Give a player in the world an item of a template in its pack (the template made up for the
    /// test world if the catalog lacks it), then have its client use it: a weapon is wielded.
    /// Returns the item.
    pub fn arm(&mut self, b: usize, character: u64, template: u32) -> u64 {
        if self.server.catalog.get(template).is_none() {
            self.server.catalog.insert(crate::test_template(
                template,
                0x1067,
                yalain_content::Props::default(),
            ));
        }
        let id = yalain_world::inventory::make_item(&mut self.server, template, 1, 0)
            .expect("the template is in the catalog");
        let p = self
            .server
            .players
            .values_mut()
            .find(|p| p.character == character)
            .expect("the player is in the world");
        let slot = u32::try_from(p.play.items.len()).unwrap_or(0);
        p.play.items.push(yalain_world::state::Held {
            id,
            template,
            slot,
            location: 0,
            quantity: 1,
        });
        if let Some(t) = self.server.things.get_mut(id) {
            t.container = Some((character, slot));
        }
        self.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(
                yalain_world::requests::function::USE,
                &[use_of(id, character)],
            ),
        );
        self.run(10);
        id
    }
}

/// A client's report of its player's place, with a heading byte and the asked motion bytes.
#[must_use]
pub fn report(
    cell: u32,
    origin: Vec3,
    heading: u8,
    motion: u32,
    teleport_stamp: u16,
) -> ClientMessage {
    ClientMessage::Position(ClientPosition {
        time: 0.0,
        cell,
        origin,
        heading_motion: u32::from(heading) << 24 | motion,
        flags: 4,
        instance_stamp: 1,
        teleport_stamp,
        force_position_stamp: 0,
        move_to_stamp: 0,
        jump_velocity: Vec3::ZERO,
    })
}

/// A call on the server with these values (packing order: the last parameter first).
#[must_use]
pub fn server_call(f: u32, args: &[Arg]) -> ClientMessage {
    ClientMessage::ServerCall(ServerCall {
        function: FunctionId(f),
        args: ScriptArgs::from_values(args).expect("packs"),
    })
}

impl<S: Store> Harness<S> {
    /// The calls a client received on its world connection since `from`: each function and its
    /// values (the last parameter first).
    #[must_use]
    pub fn calls(&self, b: usize, from: usize) -> Vec<(u32, Vec<yalain_world::calls::Value>)> {
        self.world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::PlayerCall(c) => Some((
                    c.function.0,
                    c.args
                        .as_ref()
                        .and_then(|a| yalain_world::calls::values(&a.0).ok())
                        .unwrap_or_default(),
                )),
                _ => None,
            })
            .collect()
    }

    /// The functions of the calls a client received since `from`, in order.
    #[must_use]
    pub fn functions(&self, b: usize, from: usize) -> Vec<u32> {
        self.calls(b, from).iter().map(|(f, _)| *f).collect()
    }

    /// The chat-window texts a client was shown since `from`: each kind and string id.
    #[must_use]
    pub fn texts(&self, b: usize, from: usize) -> Vec<(u32, u32)> {
        use yalain_world::calls::{Node, Value};
        self.calls(b, from)
            .iter()
            .filter(|(f, _)| *f == function::SHOW_TEXT)
            .filter_map(|(_, v)| {
                let kind = v.iter().find_map(|x| match x {
                    Value::Int(k) => Some(*k),
                    _ => None,
                })?;
                let id = v.iter().find_map(|x| match x.root() {
                    Some(Node::TextInfo(t)) => Some(t.string_id),
                    _ => None,
                })?;
                Some((kind, id))
            })
            .collect()
    }

    /// Have a client call a function on the server with these values (the last parameter first),
    /// then step `steps` times.
    pub fn ask(&mut self, b: usize, f: u32, args: &[Arg], steps: usize) {
        self.bots[b].send(true, queue::WEENIE, &server_call(f, args));
        self.run(steps);
    }
}
