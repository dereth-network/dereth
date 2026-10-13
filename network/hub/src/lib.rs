//! The Dereth Network's hub: the world registry and the links between worlds, routing tells,
//! presence, rooms and channels.
//!
//! **Depends on** `dereth-network-protocol`. **Used by** the `dereth-network-hub` binary, and in
//! tests by the conformance suite's hub checks.
//!
//! **Must never** keep or log a message body: a tell is held only while its target world is
//! reconnecting (at most a minute), a line for Discord only until it is said there, and no line,
//! presence or history is written anywhere. It binds loopback unless its operator names another
//! address.
//!
//! **Specified in** `docs/network/`. [`registry`] is the world registry (registration, keys,
//! claims, heartbeats, the federation flag, the community-list import); [`operators`] is the world
//! operators' side of it (accounts, sessions, verification, approval, reports, limits),
//! [`settings`] the hub's settings its admins change while it runs, and [`challenge`] the
//! challenges verification in game sends a world's game port;
//! [`persons`] is the person layer (characters linked to a Discord account, and person
//! friends); [`directory`] is the character directory (listed characters' profiles and
//! the public routes' answers); [`community`] reads and writes the community list; [`core`] is the
//! routing, with no I/O;
//! [`server`] is the listener that carries the HTTP API and the links, and [`api`] the operator
//! API on it; [`fetch`] makes the hub's own outbound requests, held to the addresses they may
//! reach; [`discord`] signs operators in; [`broker`] is the Discord broker (what the hub does with
//! the application's bot token, and its client for Discord's API), [`bridge`] mirrors rooms into
//! Discord servers' channels, [`rooms`] keeps which of each world's rooms carries the Global room,
//! which its mirrors follow, [`dms`] tells into a person's Discord direct messages and its replies
//! back, and [`interactions`] the bot's `/dn` command, with which a person speaks in a mirror as its
//! own character; [`forwarded`] finds a client's address behind trusted proxies; [`resolver`]
//! looks host names up, bounded; [`text`] is the rule for the text the hub keeps; [`self_update`]
//! installs the hub's own releases over its binary.

pub mod api;
pub mod bridge;
pub mod broker;
pub mod challenge;
pub mod community;
pub mod core;
pub mod directory;
pub mod discord;
pub mod dms;
pub mod fetch;
pub mod fold;
pub mod forwarded;
pub mod interactions;
pub mod operators;
pub mod persons;
pub mod registry;
pub mod resolver;
pub mod rooms;
pub mod self_update;
pub mod server;
pub mod settings;
pub mod text;
