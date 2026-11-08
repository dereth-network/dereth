//! The worlds Dereth plays through a relay on the player's own computer.
//!
//! Dereth speaks one wire format. A server that speaks another is reached through
//! `empyrean-relay`, which a launcher runs for the length of a session: the client connects to
//! the relay on the loopback address, and the relay connects to the world. The list has no place
//! for such a world, and nothing says it needs a relay, so the launcher keeps them here.
//!
//! A launcher lists them, and asks whether each is up as it asks any world, with the server-tracker
//! login in the form that wire reads ([`crate::probe::request_1999`]). No setting turns them off:
//! [`LISTED`] does, in the code.

use dereth_primitives::{EraFeatureOverrides, EraId};

use crate::state::LauncherState;
use crate::world::{AccountModel, Emulator, Endpoint, Told, World};

/// Whether a launcher lists these worlds and sends them anything.
pub const LISTED: bool = true;

/// The client the worlds take: only Dereth speaks to the relay.
const ACCEPTED_CLIENT: &str = "dereth";

/// One world reached through a relay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayedWorld {
    /// The world's key among the launcher's worlds.
    pub slug: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// The server's address, which the relay connects to.
    pub host: &'static str,
    pub port: u16,
    /// The era the relay presents.
    pub era: EraId,
    pub emulator: Emulator,
    pub account_model: AccountModel,
}

/// Every world the launcher knows this way.
pub const RELAYED_WORLDS: &[RelayedWorld] = &[RelayedWorld {
    slug: "ac1999",
    name: "AC1999",
    description: "The 1999 launch, on the ac1999 project's server. Dereth plays it through a \
                  relay on this computer.",
    host: "ac1999.cobradragon.com",
    port: 9110,
    era: EraId::Launch,
    emulator: Emulator::Gdle,
    account_model: AccountModel::AutoCreateOnFirstLogin,
}];

impl RelayedWorld {
    /// The world record the rest of the launcher works with. Its address is the server's: a
    /// launch puts the relay's in its place ([`on_relay`]).
    #[must_use]
    pub fn to_world(&self) -> World {
        let mut w = World::new(self.slug, self.name);
        w.description = Some(self.description.to_owned());
        w.emulator = self.emulator;
        w.account_model = self.account_model;
        w.era = Some(self.era.name().to_owned());
        w.era_source = Some(Told::World);
        w.era_features = Some(EraFeatureOverrides::all_of(self.era.features()).to_string());
        w.features_source = Some(Told::World);
        w.accepted_clients = vec![ACCEPTED_CLIENT.to_owned()];
        w.preferred_client = Some(ACCEPTED_CLIENT.to_owned());
        w.endpoint = Some(Endpoint {
            address: self.host.to_owned(),
            port: self.port,
            transport: None,
        });
        w.relayed = true;
        w
    }
}

/// Whether `state`'s launcher shows these worlds.
#[must_use]
pub fn shown(state: &LauncherState) -> bool {
    LISTED && !state.hide_relayed_worlds
}

/// The worlds `state`'s player is shown.
pub fn worlds(state: &LauncherState) -> impl Iterator<Item = World> + '_ {
    RELAYED_WORLDS
        .iter()
        .filter(move |_| shown(state))
        .map(RelayedWorld::to_world)
}

/// Whether `slug` is a world of this table, listed or not.
#[must_use]
pub fn is_relayed(slug: &str) -> bool {
    RELAYED_WORLDS.iter().any(|r| r.slug == slug)
}

/// `world` as the client is to reach it: at the relay on this computer, on `port`.
#[must_use]
pub fn on_relay(mut world: World, port: u16) -> World {
    world.endpoint = Some(Endpoint {
        address: std::net::Ipv4Addr::LOCALHOST.to_string(),
        port,
        transport: None,
    });
    world
}

/// The arguments that start the relay for `world`, on `port` and the port above it: towards the
/// server at `world`'s address, under `world`'s name, which names the folder the client keeps its
/// files in. It stops by itself `idle_exit` seconds after its last session, should nothing stop it
/// first. `None` for a world with no address.
#[must_use]
pub fn relay_args(world: &World, port: u16, idle_exit: u64) -> Option<Vec<String>> {
    let server = world.endpoint.as_ref()?;
    Some(vec![
        "--upstream".to_owned(),
        format!("{}:{}", server.address, server.port),
        "--port".to_owned(),
        port.to_string(),
        "--world".to_owned(),
        world.name.clone(),
        "--idle-exit".to_owned(),
        idle_exit.to_string(),
    ])
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (which worlds a launcher lists, and how it reaches them).
    use super::*;
    use crate::choices;
    use crate::datset::SetKind;
    use crate::launch::describe;

    fn slugs(state: &LauncherState) -> Vec<String> {
        let list = [World::new("coldeve", "Coldeve")];
        crate::worlds::all(&list, state).map(|w| w.slug).collect()
    }

    #[test]
    fn a_new_launcher_lists_them_with_nothing_set() {
        assert_eq!(slugs(&LauncherState::default()), ["coldeve", "ac1999"]);
    }

    /// The switch this replaced was kept in the state file, either way. What a file says of it
    /// changes nothing and is gone once the file is saved; nor can a file hide them another way.
    #[test]
    fn a_state_file_cannot_turn_them_off() {
        let dir = std::env::temp_dir().join(format!("relayed-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for word in ["true", "false"] {
            let file = dir.join(crate::state::STATE_FILE);
            std::fs::write(
                &file,
                format!(
                    r#"{{"schema": 1, "hide_relayed_worlds": true,
                         "settings": {{"relayed_worlds": {word}, "auto_update": false}}}}"#
                ),
            )
            .unwrap();
            let (state, loaded) = LauncherState::load(&dir);
            assert_eq!(loaded, crate::state::Loaded::Read);
            assert!(!state.settings.auto_update, "the file was read");
            assert_eq!(slugs(&state), ["coldeve", "ac1999"], "{word}");

            state.save(&dir).unwrap();
            let saved = std::fs::read_to_string(&file).unwrap();
            assert!(!saved.contains("relayed"), "{saved}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_test_can_leave_them_off_the_list() {
        assert_eq!(
            slugs(&crate::testing::without_relayed_worlds()),
            ["coldeve"]
        );
    }

    #[test]
    fn the_1999_world_takes_only_dereth_and_plays_the_launch_on_the_1999_files() {
        let w = RELAYED_WORLDS[0].to_world();
        assert_eq!((w.slug.as_str(), w.relayed), ("ac1999", true));
        assert!(w.accepts("dereth-0.4", None));
        assert!(!w.accepts("acclient-6096", Some("1802")));
        assert_eq!(choices::required_set_for(&w), SetKind::Original);
        let said = describe(&w);
        assert_eq!(said.era.as_deref(), Some("launch"));
        assert_eq!(
            said.era_features.unwrap(),
            dereth_primitives::EraFeatureBits::of(EraId::Launch.features()).to_string(),
            "the systems the relay's status ping names"
        );
    }

    #[test]
    fn a_launch_connects_to_the_relay_and_the_relay_to_the_server() {
        let w = RELAYED_WORLDS[0].to_world();
        let args = relay_args(&w, 9200, 120).unwrap();
        assert_eq!(args[1], "ac1999.cobradragon.com:9110");
        assert_eq!(&args[2..6], ["--port", "9200", "--world", "AC1999"]);
        let played = on_relay(w, 9200);
        let at = played.endpoint.unwrap();
        assert_eq!((at.address.as_str(), at.port), ("127.0.0.1", 9200));
    }

    #[test]
    fn the_table_knows_its_worlds_by_key_and_each_has_an_address() {
        assert!(is_relayed("ac1999"));
        assert!(!is_relayed("coldeve"));
        for r in RELAYED_WORLDS {
            assert!(!r.host.is_empty() && r.port != 0, "{}", r.slug);
        }
    }

    /// What was played on a world of this table survives the list being read again.
    #[test]
    fn what_was_played_there_is_kept_when_the_list_does_not_name_it() {
        let mut state = LauncherState::default();
        state.favorite_worlds.push("ac1999".to_owned());
        state.favorite_worlds.push("gone".to_owned());
        assert!(state.forget_unlisted(&std::collections::HashSet::new()));
        assert_eq!(state.favorite_worlds, ["ac1999"]);
    }
}
