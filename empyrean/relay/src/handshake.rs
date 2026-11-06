//! The login towards the ac1999 server, with no socket: the caller sends the datagrams this returns.

use std::net::SocketAddr;

use dereth_primitives::LocalTime;
use dereth_transport::conn::LoginState;

use crate::wire::{self, Grant, LoginMetadata};

/// A credential-bearing outbound datagram. Debug formatting never prints its bytes.
#[derive(Clone)]
pub struct Outgoing {
    pub destination: SocketAddr,
    bytes: Vec<u8>,
}

impl std::fmt::Debug for Outgoing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outgoing")
            .field("destination", &self.destination)
            .field("len", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

impl Outgoing {
    /// The datagram to send. Login bytes contain plaintext credentials; do not log them.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    AwaitingGrant,
    Granted,
    Refused,
    TimedOut,
}

/// Each direction gets a separate stream, even when both seeds are zero: the grant's first seed
/// keys what the server sends, its second what the client sends.
#[derive(Debug)]
pub struct Keys {
    pub incoming: wire::KeyStream,
    pub outgoing: wire::KeyStream,
}

/// The result of an accepted response. Duplicate grants resend the acknowledgment without replacing
/// the accepted parameters or resetting either key stream.
#[derive(Debug)]
pub enum Reply {
    Granted { acknowledgment: Outgoing },
    Duplicate { acknowledgment: Outgoing },
    Refused { string_id: u32, table_id: u32 },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("ac1999 packet: {0}")]
    Packet(#[from] wire::Error),
    #[error("response is not from the configured endpoint")]
    Endpoint,
    #[error("invalid response for the handshake")]
    Response,
    #[error("grant does not echo the requested version")]
    Version,
    #[error("handshake is no longer accepting responses")]
    State,
    #[error("login timed out")]
    Timeout,
    #[error("invalid local clock")]
    Clock,
}

/// A bounded login exchange: no sockets, no credential logging, no account creation. Retries reuse
/// the same request and use caller-supplied monotonic time.
pub struct Handshake {
    endpoint: SocketAddr,
    version: String,
    request: Option<Outgoing>,
    login: LoginState,
    last_poll: Option<LocalTime>,
    state: State,
    accepted: Option<Grant>,
    acknowledgment: Option<Outgoing>,
    keys: Option<Keys>,
}

impl std::fmt::Debug for Handshake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handshake")
            .field("endpoint", &self.endpoint)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl Handshake {
    /// One login for `account:password`, reporting the file iterations in `metadata`.
    ///
    /// # Errors
    /// Credentials the 1999 login cannot carry.
    pub fn new(
        endpoint: SocketAddr,
        account: &str,
        password: &str,
        metadata: LoginMetadata,
    ) -> Result<Self, Error> {
        let version = wire::CLIENT_VERSION.to_owned();
        let bytes = wire::login_request(&version, account, password, metadata)?;
        Ok(Self {
            endpoint,
            version,
            request: Some(Outgoing {
                destination: endpoint,
                bytes,
            }),
            login: LoginState::new(),
            last_poll: None,
            state: State::AwaitingGrant,
            accepted: None,
            acknowledgment: None,
            keys: None,
        })
    }

    #[must_use]
    pub fn state(&self) -> State {
        self.state
    }

    #[must_use]
    pub fn accepted(&self) -> Option<&Grant> {
        self.accepted.as_ref()
    }

    /// Transfer the initialized streams to the data connection exactly once.
    pub fn take_keys(&mut self) -> Option<Keys> {
        self.keys.take()
    }

    /// An initial or retry login datagram, at most twenty attempts two seconds apart. After a
    /// grant, refusal or timeout there are no more credential-bearing sends.
    ///
    /// # Errors
    /// [`Error::Timeout`] after the last unanswered attempt, or [`Error::Clock`] for a nonfinite or
    /// backwards clock.
    pub fn request(&mut self, now: LocalTime) -> Result<Option<Outgoing>, Error> {
        if !now.0.is_finite() || self.last_poll.is_some_and(|last| now.0 < last.0) {
            return Err(Error::Clock);
        }
        self.last_poll = Some(now);
        if self.state != State::AwaitingGrant {
            return Ok(None);
        }
        match self.login.should_send(now) {
            Ok(true) => Ok(self.request.clone()),
            Ok(false) => Ok(None),
            Err(_) => {
                self.state = State::TimedOut;
                self.request = None;
                Err(Error::Timeout)
            }
        }
    }

    /// Validate one grant or refusal and produce its acknowledgment. The endpoint is pinned.
    ///
    /// # Errors
    /// Wrong endpoint, bad framing or checksum, an incompatible grant, or a finished handshake.
    /// Invalid responses change nothing.
    pub fn receive(&mut self, bytes: &[u8], from: SocketAddr) -> Result<Reply, Error> {
        if from != self.endpoint {
            return Err(Error::Endpoint);
        }
        if matches!(self.state, State::Refused | State::TimedOut) || self.login.requests_sent == 0 {
            return Err(Error::State);
        }
        let packet = wire::Packet::decode(bytes)?;
        // The server refuses a login with a network error carrying a string id.
        if packet.header.has(wire::flags::NET_ERROR) && !packet.header.has(wire::flags::GRANT) {
            if !packet.generic_checksum_ok() {
                return Err(wire::Error::Checksum.into());
            }
            if self.state != State::AwaitingGrant {
                return Err(Error::State);
            }
            let body: &[u8; 8] = packet.body.try_into().map_err(|_| Error::Response)?;
            self.state = State::Refused;
            self.request = None;
            return Ok(Reply::Refused {
                string_id: u32::from_le_bytes(body[..4].try_into().map_err(|_| Error::Response)?),
                table_id: u32::from_le_bytes(body[4..].try_into().map_err(|_| Error::Response)?),
            });
        }
        let grant = Grant::decode(packet)?;
        if !grant.accepts_version(&self.version) {
            return Err(Error::Version);
        }
        let acknowledgment = Outgoing {
            destination: from,
            bytes: grant.acknowledgment()?,
        };
        if let Some(previous) = &self.accepted {
            if previous != &grant {
                return Err(Error::Response);
            }
            return Ok(Reply::Duplicate { acknowledgment });
        }
        self.keys = Some(Keys {
            incoming: wire::KeyStream::new(grant.server_to_client_seed),
            outgoing: wire::KeyStream::new(grant.client_to_server_seed),
        });
        self.accepted = Some(grant);
        self.acknowledgment = Some(acknowledgment.clone());
        self.state = State::Granted;
        self.request = None;
        Ok(Reply::Granted { acknowledgment })
    }

    /// Repeat the accepted acknowledgment if the caller's retry timer expires. It contains no
    /// credentials.
    #[must_use]
    pub fn acknowledgment(&self) -> Option<Outgoing> {
        self.acknowledgment.clone()
    }
}

#[cfg(test)]
mod tests;
