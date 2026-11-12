//! The hub's integration tests: one binary. The self-update test writes a hub and starts it in this
//! process, so every process these tests start is started as the updater starts one
//! ([`Start`]): one started any other way while the hub is being written could still be holding it
//! open when it starts, which Linux refuses.

mod broker;
mod community;
mod conformance;
mod deploy;
mod directory;
mod hardening;
mod in_game;
mod links;
mod operators;
mod persons;
mod registry_api;
mod self_update;

/// Starting a process through the updater ([`dereth_server_updater::program`]).
trait Start {
    /// [`std::process::Command::spawn`].
    fn start(&mut self) -> std::io::Result<std::process::Child>;
    /// [`std::process::Command::output`].
    fn run(&mut self) -> std::io::Result<std::process::Output>;
}

impl Start for std::process::Command {
    fn start(&mut self) -> std::io::Result<std::process::Child> {
        dereth_server_updater::program::spawn(self)
    }

    fn run(&mut self) -> std::io::Result<std::process::Output> {
        dereth_server_updater::program::output(self)
    }
}
