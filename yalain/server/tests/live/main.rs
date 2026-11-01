//! Our own client against this server's executable on loopback: the executable started with the
//! September 2013 install serving a small world database written for it, our client on a UDP socket
//! of this process playing a whole session through it. Real sockets and the real clock: each
//! step waits for what it should leave in the client's model, with a time limit only against a
//! hang. Fixture: the built executable and the install.

mod reload;
mod relogin;
mod session;
