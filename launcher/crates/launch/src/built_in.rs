//! The worlds a launcher lists of its own, which no list it reads names and which its clients reach
//! directly. (A world reached through a relay is [`crate::relayed`]'s.)
//!
//! Asheron's Call 2 has no community list, so the worlds of its realm that are not on the Dereth
//! Network's hub are kept here. Each is listed until the list the launcher reads names it (the
//! hub's, once the world is registered there): then the list's row is shown in its place, with
//! what the hub says of it. Nothing else turns one off but this table.
//!
//! A launcher asks such a world whether it is up as it asks any world that takes no status ping:
//! with the server-tracker login ([`crate::probe`]).

use dereth_primitives::EraId;

use crate::state::LauncherState;
use crate::world::{AccountModel, Emulator, Endpoint, Game, Told, World};

/// One world the launcher knows of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltInWorld {
    /// The world's key among the launcher's worlds.
    pub slug: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub game: Game,
    /// Where its clients connect.
    pub host: &'static str,
    pub port: u16,
    pub emulator: Emulator,
    /// How settled the world is: `Stable`, `Development` or `Experimental`.
    pub development_status: &'static str,
    pub account_model: AccountModel,
}

/// Every world the launcher knows this way.
pub const BUILT_IN_WORLDS: &[BuiltInWorld] = &[BuiltInWorld {
    slug: "icefire",
    name: "Icefire",
    description: "An Asheron's Call 2 world on the Icefire server. Sign in with your Icefire \
                  account.",
    game: Game::Auberean,
    host: "icefire.portalspace.net",
    port: 8610,
    emulator: Emulator::Icefire,
    development_status: "Development",
    account_model: AccountModel::Unknown,
}];

impl BuiltInWorld {
    /// The world record the rest of the launcher works with. A world of Asheron's Call 2 plays the
    /// end of retail, and is played with Auberean or that game's retail client, as every world of
    /// its realm is.
    #[must_use]
    pub fn to_world(&self) -> World {
        let mut w = World::new(self.slug, self.name);
        w.game = self.game;
        w.description = Some(self.description.to_owned());
        w.emulator = self.emulator;
        w.development_status = Some(self.development_status.to_owned());
        w.account_model = self.account_model;
        w.era = Some(EraId::Eor.name().to_owned());
        w.era_source = Some(Told::World);
        w.endpoint = Some(Endpoint {
            address: self.host.to_owned(),
            port: self.port,
            transport: None,
        });
        w
    }
}

impl BuiltInWorld {
    /// Whether `listed`, a world of the list, is this one: of its game, and kept under its key or
    /// its hub slug, or at its address and port.
    #[must_use]
    pub fn is_listed_as(&self, listed: &World) -> bool {
        listed.game == self.game
            && (listed.slug == self.slug
                || listed.hub_slug.as_deref() == Some(self.slug)
                || listed.endpoint.as_ref().is_some_and(|e| {
                    e.address.eq_ignore_ascii_case(self.host) && e.port == self.port
                }))
    }
}

/// The worlds `state`'s player is shown beside the list's worlds (`list`): those the list does
/// not name, unless the state keeps the launcher's own worlds off the list
/// ([`LauncherState::hide_built_in_worlds`]).
pub fn worlds<'a>(list: &'a [World], state: &'a LauncherState) -> impl Iterator<Item = World> + 'a {
    BUILT_IN_WORLDS
        .iter()
        .filter(move |_| !state.hide_built_in_worlds)
        .filter(move |b| !list.iter().any(|w| b.is_listed_as(w)))
        .map(BuiltInWorld::to_world)
}

/// Whether `slug` is a world of this table.
#[must_use]
pub fn is_built_in(slug: &str) -> bool {
    BUILT_IN_WORLDS.iter().any(|w| w.slug == slug)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (which worlds a launcher lists, and what it does with them).
    use super::*;
    use crate::auberean::{auberean_plan, retail_plan};
    use std::path::Path;

    fn icefire() -> World {
        BUILT_IN_WORLDS
            .iter()
            .find(|w| w.slug == "icefire")
            .expect("in the table")
            .to_world()
    }

    fn slugs(state: &LauncherState) -> Vec<String> {
        let list = [World::new("coldeve", "Coldeve")];
        crate::worlds::all(&list, state).map(|w| w.slug).collect()
    }

    /// Once the hub lists Icefire, its row is shown in the built-in one's place, once, with what the
    /// hub says of it; while the hub doesn't, the built-in row is.
    #[test]
    fn icefire_gives_way_to_the_hubs_row_for_it() {
        let page = |rows: &str| format!(r#"{{"data": [{rows}]}}"#);
        let icefire_row = r#"{"slug": "icefire", "name": "Icefire", "game": "auberean",
            "endpoint": {"address": "icefire.portalspace.net", "port": 8610},
            "emulator": {"family": "Icefire"}, "maturity": "Development"}"#;
        let coldeve_row = r#"{"slug": "coldeve", "name": "Coldeve",
            "endpoint": {"address": "play.example.org", "port": 9000}}"#;
        let shown = |rows: &str| -> Vec<World> {
            let list = crate::serverlist::parse_hub_pages(&[page(rows)]).unwrap();
            crate::worlds::all(&list, &LauncherState::default())
                .filter(|w| w.slug != "ac1999")
                .collect()
        };

        let without = shown(coldeve_row);
        assert_eq!(
            without.iter().map(|w| w.slug.as_str()).collect::<Vec<_>>(),
            ["coldeve", "icefire"]
        );
        assert_eq!(without[1].hub_slug, None, "the built-in row");

        let with = shown(&format!("{coldeve_row}, {icefire_row}"));
        let icefire: Vec<&World> = with.iter().filter(|w| w.slug == "icefire").collect();
        let [hubs] = icefire[..] else {
            panic!("{with:?}");
        };
        assert_eq!(hubs.hub_slug.as_deref(), Some("icefire"), "the hub's row");
        assert_eq!(
            (hubs.game, hubs.emulator),
            (Game::Auberean, Emulator::Icefire)
        );
    }

    /// The hub's row is the built-in world's when it is of its game and has its key, its hub slug,
    /// or its address and port, however the address is spelt; a world of the other game is not.
    #[test]
    fn a_listed_world_is_the_built_in_one_by_its_key_its_hub_slug_or_its_address() {
        let ours = BUILT_IN_WORLDS[0];
        let listed = |slug: &str, hub_slug: Option<&str>, at: Option<(&str, u16)>| {
            let mut w = World::new(slug, slug);
            w.game = Game::Auberean;
            w.hub_slug = hub_slug.map(str::to_owned);
            w.endpoint = at.map(|(address, port)| Endpoint {
                address: address.to_owned(),
                port,
                transport: None,
            });
            w
        };
        assert!(ours.is_listed_as(&listed("icefire", None, None)));
        assert!(ours.is_listed_as(&listed("icefire-ac2", Some("icefire"), None)));
        assert!(ours.is_listed_as(&listed(
            "frost",
            Some("frost"),
            Some(("IceFire.PortalSpace.net", 8610))
        )));
        assert!(!ours.is_listed_as(&listed(
            "frost",
            Some("frost"),
            Some(("icefire.portalspace.net", 8611))
        )));
        let mut other_game = listed("icefire", Some("icefire"), None);
        other_game.game = Game::Dereth;
        assert!(!ours.is_listed_as(&other_game));
    }

    #[test]
    fn a_new_launcher_lists_icefire_after_the_list_and_the_relayed_worlds() {
        assert_eq!(
            slugs(&LauncherState::default()),
            ["coldeve", "ac1999", "icefire"]
        );
        assert_eq!(
            slugs(&crate::testing::without_built_in_worlds()),
            ["coldeve"]
        );
    }

    /// Icefire: an Asheron's Call 2 world on its own server, reached directly, still in
    /// development, playing the end of retail.
    #[test]
    fn icefire_is_an_asherons_call_2_world_reached_directly_at_its_own_address() {
        let w = icefire();
        assert_eq!((w.name.as_str(), w.game), ("Icefire", Game::Auberean));
        let at = w.endpoint.as_ref().unwrap();
        assert_eq!(
            (at.address.as_str(), at.port),
            ("icefire.portalspace.net", 8610)
        );
        assert!(!w.relayed, "no relay: its clients speak its protocol");
        assert_eq!(w.emulator, Emulator::Icefire);
        assert_eq!(w.development_status.as_deref(), Some("Development"));
        assert_eq!(
            (w.era.as_deref(), w.era_source),
            (Some("eor"), Some(Told::World))
        );
        assert_eq!(
            w.account_model,
            AccountModel::Unknown,
            "the player's own Icefire account: the first login makes none"
        );
        assert_eq!(w.status_url, None);
        assert_eq!(w.websocket_url, None);
    }

    /// Only a Yalain world is told Auberean keeps its overlay or asked for a friends service, and
    /// only a world that writes into the retail client's install is played from a private copy of
    /// it. Icefire is none of those: its clients start as the retail client would.
    #[test]
    fn icefire_gets_none_of_a_yalain_worlds_switches_and_no_private_copy() {
        let w = icefire();
        assert!(!crate::auberean::is_yalain(&w));
        assert!(!w.writes_retail_install());
        let server = "127.0.0.1:8610".parse().unwrap();
        let auberean = auberean_plan(
            Path::new("auberean"),
            Path::new("ac2"),
            server,
            "player",
            None,
            crate::auberean::is_yalain(&w),
        )
        .argv("secret");
        let retail = retail_plan(
            Path::new("AC2client.exe"),
            server,
            "player",
            None,
            w.writes_retail_install(),
        )
        .argv("secret");
        for argv in [&auberean, &retail] {
            for yalain_only in ["--world-overlays", "--gunserver", "--gunticket"] {
                assert!(!argv.iter().any(|a| a == yalain_only), "{argv:?}");
            }
        }
        assert_eq!(
            auberean,
            [
                "--dats",
                "ac2",
                "--host",
                "127.0.0.1",
                "--port",
                "8610",
                "--account",
                "player",
                "--glsticketdirect",
                "secret"
            ]
        );
        assert_eq!(retail[0], "--rodat", "the player's install, read only");
    }

    #[test]
    fn the_table_knows_its_worlds_by_key_and_each_has_an_address() {
        assert!(is_built_in("icefire"));
        assert!(!is_built_in("ac1999"));
        for w in BUILT_IN_WORLDS {
            assert!(!w.host.is_empty() && w.port != 0, "{}", w.slug);
            assert!(!crate::relayed::is_relayed(w.slug), "{}", w.slug);
        }
    }

    /// What was played on a world of this table survives the list being read again.
    #[test]
    fn what_was_played_there_is_kept_when_the_list_does_not_name_it() {
        let mut state = LauncherState::default();
        state.favorite_worlds.push("icefire".to_owned());
        state.favorite_worlds.push("gone".to_owned());
        assert!(state.forget_unlisted(&std::collections::HashSet::new()));
        assert_eq!(state.favorite_worlds, ["icefire"]);
    }
}
