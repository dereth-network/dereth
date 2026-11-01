//! The friends library the retail Asheron's Call 2 client loads in place of its original: the client's friends interfaces, served over the friends stream.
//!
//! **Depends on** the friends stream (`auberean-friends`): its client session, its elements and its
//! codes. **Used by** the retail client of 30 September 2013 (00.00.00.0930), which loads the library
//! built for `i686-pc-windows-msvc` by the path its `--gunclientdll` option names; the launcher
//! ships it beside itself and passes that option. No crate depends on it.
//!
//! **Must never** fail the game: every entry answers at once, a panic becomes an error code, and
//! nothing the client calls waits on the network. It calls the client back only inside the
//! client's pump, on the client's thread, never holding a lock while it does; it writes nothing to
//! the registry and nothing into the client's install, and its log (the file
//! `AUBEREAN_FRIENDS_LOG` names) never holds the ticket or a tell's text.
//!
//! **What the client needs.** The client creates one object and queries it for ten interfaces
//! ([`ids`]). It calls nineteen entries on five of them: on the connection, set up, connect,
//! disconnect and the pump; a tell (a line to a room answers not implemented); the friends list,
//! adding and removing a friend, the ignore list, adding and removing a name, the start-of-session
//! flags and the player's state; the ticket (asked for, set, presented, released); and a help call
//! (not implemented). Every request is answered later, through the pump, by one result carrying the
//! client's own context. The client waits for one connect event after each connect, and presents
//! its ticket after each, the first and every reconnect.
//!
//! | part | what |
//! |---|---|
//! | [`engine`] | the state, with no I/O: the session, the contexts, the ticket and the connect events |
//! | [`layout`] | each result and event as the client reads it, with 32-bit pointers |
//! | [`net`] | the sockets, each connection on threads of its own |
//! | [`host`] | the engine with its sockets and the real clock |
//! | [`ids`] | the class and the ten interfaces' identifiers |
//! | `com` | the class factory, the object and its tables (32-bit Windows only) |
//!
//! Everything but `com` builds and is tested on any host; the library itself is built for
//! `i686-pc-windows-msvc` with the C runtime inside (`-C target-feature=+crt-static`), so that it
//! needs no Visual C++ runtime on the player's machine or under Wine:
//!
//! ```text
//! RUSTFLAGS=-Ctarget-feature=+crt-static cargo build -p auberean-friends-dll --release --target i686-pc-windows-msvc
//! ```
//!
//! Its tests on that target also load the built library and call it as the retail client does
//! (`cargo test -p auberean-friends-dll --target i686-pc-windows-msvc`). The example
//! `friends-world` serves the friends stream alone, for trying the library with the retail client
//! beside a server that has no friends service.

pub mod engine;
pub mod host;
pub mod ids;
pub mod layout;
pub mod log;
pub mod net;

#[cfg(all(windows, target_arch = "x86"))]
mod com;
