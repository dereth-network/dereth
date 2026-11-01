//! The Dereth Network end to end on loopback: the network's hub, a world of the first game
//! (Empyrean) and two of this game's worlds (this server's executable), each a program of its own
//! with its files in a scratch folder, linked through the hub; in each world a player of that
//! game's own client, and beside each of this game's players its friends stream.
//!
//! Real sockets and the real clock, every port the system's: each step waits for what it should
//! bring (a line a client shows, a frame the hub's wire log records, a line a program logs), with
//! one limit only against a hang. Every program is stopped however the run ends.
//!
//! Fixture: the four programs, built beside this binary or named by `DERETH_NETWORK_HUB_BIN`,
//! `EMPYREAN_SERVER_BIN`, `EMPYREAN_IMPORT_BIN` and `YALAIN_BIN` (`cargo xtask gate
//! auberean-network` builds them first); the September 2013 install and the authored world; the
//! first game's data files; and the first game's world database, `EMPYREAN_TEST_WORLD_PACK`, else
//! built by the importer from the release it keeps in its cache.

mod checks;
mod end_to_end;
mod first_game;
mod programs;
mod run;
mod second_game;
