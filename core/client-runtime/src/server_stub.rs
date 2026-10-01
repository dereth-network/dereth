//! The answers a headless client with no server gives itself, in the server's place.
//!
//! Some requests hold client state until the server answers them: the allegiance panel's request
//! holds the busy count, and with it the hourglass pointer, until `0x0020
//! Allegiance_AllegianceUpdate` arrives. A shard always answers it. A headless client with no
//! server never would, and would show the hourglass for the rest of its run.
//!
//! [`ServerStub`](crate::server_stub::ServerStub) stands in for a shard with nothing to tell. Each
//! frame it reads what the client sent and answers what a shard always answers, as a shard whose
//! character has no allegiance would. The answers are applied to the game model the way the
//! arriving message would be, after the request has gone out, so the client sees them on the next
//! frame.
//!
//! The answers are kept to the ones a client would otherwise wait on forever. Each is its own
//! switch, so a run that is about the waiting can turn that one answer off and give it by hand.

use dereth_client_model::{Request, World};

/// A stand-in for a server's answers. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerStub {
    /// Answer an allegiance request (`0x001F Allegiance_UpdateRequest` with its switch on) with
    /// an empty `0x0020 Allegiance_AllegianceUpdate`.
    ///
    /// Only while the client holds no allegiance of its own: a run that has given the client an
    /// allegiance is speaking for the server itself, and an empty answer would erase it.
    pub allegiance: bool,
    /// How many answers this stub has given, of every kind.
    pub answered: u64,
}

impl Default for ServerStub {
    fn default() -> Self {
        Self {
            allegiance: true,
            answered: 0,
        }
    }
}

impl ServerStub {
    /// A stub that answers nothing: the starting point for a run that turns on one answer.
    #[must_use]
    pub fn silent() -> Self {
        Self {
            allegiance: false,
            answered: 0,
        }
    }

    /// Answer what `sent` asked for, into `world`.
    pub fn answer(&mut self, sent: &[Request], world: &mut World) {
        for request in sent {
            if let Request::AllegianceUpdateRequest(m) = request {
                if self.allegiance && m.on_off != 0 && holds_no_allegiance(world) {
                    world.handle_allegiance_update(
                        &dereth_protocol::social::AllegianceProfile::default(),
                    );
                    self.answered += 1;
                }
            }
        }
    }
}

/// Whether the client knows of no allegiance at all: no members, no name and no message of the
/// day.
fn holds_no_allegiance(world: &World) -> bool {
    let a = &world.allegiance;
    a.data.is_empty()
        && a.monarch.is_none()
        && a.allegiance_name.is_empty()
        && a.motd.is_empty()
        && a.officers.is_empty()
}
