//! The session recorder: a loopback proxy between a game client and its server that writes every
//! datagram to a capture file, and the reader that lists what one object was sent.
//!
//! **Depends on** the transport (`dereth-transport`), for the referral the recorder points at
//! itself; the capture index (`dereth-pcap`), whose capture reader, session router, reassembly and
//! first-game decoder read a recording; and the second game's messages (`auberean-protocol`) and
//! the object each is about (`auberean-client-model`). **Used by** nothing: it is a tool, the
//! `dereth-recorder` binary (`record`, `read`) over this library.
//!
//! **Must never** listen anywhere but loopback, write anywhere but the file it is given, or change
//! a datagram on its way but the referral that would take the client past it.
//!
//! [`record`] is the proxy, [`capture`] the file it writes (pcap) and [`read`] the reader.

pub mod capture;
pub mod read;
pub mod record;

#[cfg(test)]
mod testing;
