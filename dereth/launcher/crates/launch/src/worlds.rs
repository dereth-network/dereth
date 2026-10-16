//! The worlds a launcher shows: the world list's, then the servers the player added, each as
//! it last said of itself (its status, or its heartbeat to the hub).
//!
//! Every front end over this crate (the launcher's window, a web page) shows a world this way, so
//! a world reads the same in each.

use crate::hubstatus::HubStatus;
use crate::state::LauncherState;
use crate::status::LiveStatus;
use crate::world::{AccountModel, Emulator, Told, World, WorldState};

/// Every world: the list's, then the servers the player added, as their rows say.
pub fn all<'a>(list: &'a [World], state: &'a LauncherState) -> impl Iterator<Item = World> + 'a {
    list.iter()
        .cloned()
        .chain(state.custom_worlds.iter().map(|c| c.to_world()))
}

/// Whether a server the player added (`added`, as shown) is a world of the list (`listed`, as
/// shown): one status document, or one WebSocket endpoint ([`crate::direct`]). Such a server is
/// shown once, as the list's world.
#[must_use]
pub fn same_world(listed: &World, added: &World) -> bool {
    let one = |a: &Option<String>, b: &Option<String>, same: fn(&str, &str) -> bool| matches!((a, b), (Some(a), Some(b)) if same(a, b));
    one(
        &listed.status_url,
        &added.status_url,
        crate::direct::same_status,
    ) || one(
        &listed.websocket_url,
        &added.websocket_url,
        crate::direct::same_websocket,
    )
}

/// `w` as it is shown, with what it says of itself (`live`: its state, players, software, era,
/// systems and files).
///
/// It plays the era it says; else the one its row gives it (the launcher's table of worlds that
/// run a client of their own speaks for the world there); else the end of retail, which is no era
/// named: the Dereth client then plays the end of retail from the later files its world is drawn
/// from. Its systems are the ones it names over that era's table, or the era's whole table.
///
/// Whether it is up and how many are on is the world's own word (`live`) when it has one; else
/// the hub's from the world's heartbeat (`hub`, while it counts), for a world of the list; else
/// whether it answered a probe (`probed`).
///
/// A browser reaches it at the WebSocket endpoint the hub names while the world is linked, or a
/// server the player added at the one it was added by or its status document names.
#[must_use]
pub fn shown(
    mut w: World,
    probed: Option<bool>,
    live: Option<&LiveStatus>,
    hub: Option<&HubStatus>,
    state: &LauncherState,
) -> World {
    // The hub speaks for the worlds it lists, never for a server the player added.
    let hub = hub.filter(|_| !state.custom_worlds.iter().any(|c| c.slug == w.slug));
    // Where a browser reaches it: the hub's latest word, which a link coming or going since the
    // list was read changes; else the list's own; else, for a world with none, its status
    // document's.
    if let Some(h) = hub {
        w.websocket_url.clone_from(&h.websocket_url);
        if h.status_url.is_some() {
            w.status_url.clone_from(&h.status_url);
        }
    }
    if w.websocket_url.is_none() {
        w.websocket_url = live.and_then(|l| l.websocket_url.clone());
    }
    if live.is_none() {
        if let Some(h) = hub {
            w.state = h.state;
            w.players = h.players;
        } else if let Some(up) = probed {
            w.state = if up {
                WorldState::Online
            } else {
                WorldState::Offline
            };
        }
    }
    if w.era_features.is_some() {
        w.features_source = Some(Told::World);
    }
    if let Some(l) = live {
        // Only Empyrean publishes the document and answers the status ping.
        if l.software
            .as_deref()
            .is_none_or(|s| s.eq_ignore_ascii_case("Empyrean"))
        {
            w.emulator = Emulator::Empyrean;
        }
        // A server the player added without naming it is called what it calls itself.
        if let Some(name) = &l.world_name {
            if state
                .custom_worlds
                .iter()
                .any(|c| c.slug == w.slug && c.name == c.host)
            {
                w.name.clone_from(name);
            }
        }
        if l.version.is_some() {
            w.emulator_version.clone_from(&l.version);
        }
        w.state = l.state;
        if l.players.is_some() {
            w.players = l.players;
        }
        if l.patching.is_some() {
            w.dats.patches_over_wire = l.patching;
        }
        // An era it names is its own, with the systems it names over that era's table, or that
        // era's whole table: never systems said for another era.
        if l.era.is_some() {
            w.era.clone_from(&l.era);
            w.era_source = Some(Told::World);
            w.era_features.clone_from(&l.era_features);
            w.features_source = l.era_features.as_ref().map(|_| Told::World);
        } else if l.era_features.is_some() {
            w.era_features.clone_from(&l.era_features);
            w.features_source = Some(Told::World);
        }
        if w.account_model == AccountModel::Unknown && l.auto_create_accounts == Some(true) {
            w.account_model = AccountModel::AutoCreateOnFirstLogin;
        }
    }
    w
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's worlds as shown: their own word, the hub's, the probe's).
    use super::*;
    use crate::world::Emulator;

    #[test]
    fn a_world_is_shown_with_its_status_over_its_row() {
        let mut state = LauncherState::default();
        let slug = state
            .add_custom_world("", "play.example.org", "9000", None, Emulator::Unknown)
            .unwrap();
        let listed = World::new("coldeve", "Coldeve");
        let worlds: Vec<World> = all(std::slice::from_ref(&listed), &state).collect();
        assert_eq!(
            worlds.iter().map(|w| w.slug.as_str()).collect::<Vec<_>>(),
            ["coldeve", slug.as_str()],
            "the list's, then the player's"
        );

        // Nothing said: no era named, which is the end of retail.
        let w = shown(worlds[1].clone(), None, None, None, &state);
        assert_eq!((w.era.as_deref(), w.era_source), (None, None));
        assert_eq!(w.state, WorldState::Unknown, "no status: unknown");

        // A status: its era wins, it names the server, and it is Empyrean.
        let live = LiveStatus {
            state: WorldState::Online,
            players: Some(3),
            patching: Some(true),
            era: Some("eor".into()),
            era_features: Some("trade=false".into()),
            world_name: Some("Eulmore".into()),
            ..LiveStatus::default()
        };
        let w = shown(worlds[1].clone(), None, Some(&live), None, &state);
        assert_eq!(
            w.name, "Eulmore",
            "an unnamed server is called what it calls itself"
        );
        assert_eq!(w.emulator, Emulator::Empyrean);
        assert_eq!(
            (w.era.as_deref(), w.era_source),
            (Some("eor"), Some(Told::World))
        );
        assert_eq!(w.features_source, Some(Told::World));
        assert_eq!((w.state, w.players), (WorldState::Online, Some(3)));
        assert!(w.patches());

        // A probe alone says up or down.
        let w = shown(listed, Some(false), None, None, &state);
        assert_eq!(w.state, WorldState::Offline);
    }

    /// The hub's word for a world from its heartbeat, counting until `expires_at`.
    fn hub(state: WorldState, players: Option<u32>) -> HubStatus {
        HubStatus {
            state,
            players,
            expires_at: 1_000,
            websocket_url: None,
            status_url: None,
        }
    }

    /// A browser reaches a world of the list at the WebSocket endpoint the hub's latest word
    /// names, which may take the list's own away; a server the player added, at the one it was
    /// added by, or its status document's; and nothing else.
    #[test]
    fn a_world_is_reached_directly_where_the_hub_or_its_own_status_says() {
        let mut state = LauncherState::default();
        let mut listed = World::new("ironwood", "Ironwood");
        listed.websocket_url = Some("wss://ironwood.example.org/old".into());
        let linked = HubStatus {
            websocket_url: Some("wss://ironwood.example.org/ws".into()),
            status_url: Some("https://ironwood.example.org/status".into()),
            ..hub(WorldState::Online, Some(3))
        };
        let w = shown(listed.clone(), None, None, Some(&linked), &state);
        assert_eq!(
            (w.websocket_url.as_deref(), w.status_url.as_deref()),
            (
                Some("wss://ironwood.example.org/ws"),
                Some("https://ironwood.example.org/status")
            )
        );
        let unlinked = hub(WorldState::Online, Some(3));
        let w = shown(listed.clone(), None, None, Some(&unlinked), &state);
        assert_eq!(w.websocket_url, None, "the hub names none now");
        let w = shown(listed, None, None, None, &state);
        assert_eq!(
            w.websocket_url.as_deref(),
            Some("wss://ironwood.example.org/old"),
            "without the hub's word, the list's"
        );

        let by_ws = state
            .add_custom_world("", "play.example.org", "9000", None, Emulator::Unknown)
            .unwrap();
        state.set_custom_world_url(&by_ws, Some("wss://play.example.org/ws"));
        let by_status = state
            .add_custom_world("", "other.example.org", "9000", None, Emulator::Unknown)
            .unwrap();
        state.set_custom_world_url(&by_status, Some("https://other.example.org/status"));
        let added: Vec<World> = state.custom_worlds.iter().map(|c| c.to_world()).collect();
        let w = shown(added[0].clone(), None, None, Some(&linked), &state);
        assert_eq!(
            w.websocket_url.as_deref(),
            Some("wss://play.example.org/ws"),
            "the hub never speaks for a server the player added"
        );
        let w = shown(added[1].clone(), None, None, None, &state);
        assert_eq!(
            (w.websocket_url.as_deref(), w.status_url.as_deref()),
            (None, Some("https://other.example.org/status"))
        );
        let says = LiveStatus {
            websocket_url: Some("wss://other.example.org/ws".into()),
            ..LiveStatus::default()
        };
        let w = shown(added[1].clone(), None, Some(&says), None, &state);
        assert_eq!(
            w.websocket_url.as_deref(),
            Some("wss://other.example.org/ws")
        );
    }

    /// A server the player added is a world of the list when the two name one status document or
    /// one WebSocket endpoint, however each spells it; an address only one of them names makes
    /// nothing one.
    #[test]
    fn a_server_added_is_a_listed_world_by_its_status_document_or_its_websocket() {
        let mut listed = World::new("ironwood", "Ironwood");
        listed.websocket_url = Some("wss://ironwood.example.org/ws".into());
        listed.status_url = Some("https://ironwood.example.org/status".into());
        let mut added = World::new("custom-1", "ironwood.example.org");
        assert!(!same_world(&listed, &added));
        added.status_url = Some("https://Ironwood.example.org/".into());
        assert!(same_world(&listed, &added), "one status document");
        added.status_url = Some("https://ironwood.example.org/other".into());
        assert!(!same_world(&listed, &added));
        added.websocket_url = Some("wss://ironwood.example.org:443/ws/".into());
        assert!(same_world(&listed, &added), "one WebSocket endpoint");
        listed.websocket_url = None;
        listed.status_url = None;
        assert!(!same_world(&listed, &added), "the list names neither");
    }

    /// Each step wins where the one before it says nothing: the world's own answer, then the
    /// hub's word from its heartbeat, then the probe.
    #[test]
    fn the_worlds_own_answer_goes_before_the_hubs_word_and_the_hubs_before_the_probe() {
        let state = LauncherState::default();
        let mut listed = World::new("frostfell", "Frostfell");
        listed.emulator = Emulator::Empyrean;
        let own = LiveStatus {
            state: WorldState::Starting,
            players: Some(1),
            ..LiveStatus::default()
        };
        let up = hub(WorldState::Online, Some(30));

        let w = shown(listed.clone(), Some(false), Some(&own), Some(&up), &state);
        assert_eq!(
            (w.state, w.players),
            (WorldState::Starting, Some(1)),
            "the world's own answer over the hub's"
        );
        let w = shown(listed.clone(), Some(false), None, Some(&up), &state);
        assert_eq!(
            (w.state, w.players),
            (WorldState::Online, Some(30)),
            "the hub's over the probe"
        );
        let down = hub(WorldState::Offline, None);
        let w = shown(listed.clone(), Some(true), None, Some(&down), &state);
        assert_eq!((w.state, w.players), (WorldState::Offline, None));
        let w = shown(listed.clone(), Some(true), None, None, &state);
        assert_eq!(
            (w.state, w.players),
            (WorldState::Online, None),
            "the probe when the hub says nothing"
        );
        let w = shown(listed, None, None, None, &state);
        assert_eq!(w.state, WorldState::Unknown);
    }

    #[test]
    fn an_ace_world_shows_the_players_its_heartbeat_reported_to_the_hub() {
        let mut ace = World::new("coldeve", "Coldeve");
        ace.emulator = Emulator::Ace;
        let w = shown(
            ace,
            Some(true),
            None,
            Some(&hub(WorldState::Online, Some(57))),
            &LauncherState::default(),
        );
        assert_eq!((w.state, w.players), (WorldState::Online, Some(57)));
        assert_eq!(
            w.emulator,
            Emulator::Ace,
            "the hub does not make it Empyrean"
        );
        assert_eq!(w.era, None, "and names no era for it");
    }

    #[test]
    fn a_server_the_player_added_never_takes_the_hubs_word() {
        let mut state = LauncherState::default();
        let slug = state
            .add_custom_world("Coldeve", "play.example.org", "9000", None, Emulator::Ace)
            .unwrap();
        let added = all(&[], &state).next().unwrap();
        let w = shown(
            added,
            Some(false),
            None,
            Some(&hub(WorldState::Online, Some(57))),
            &state,
        );
        assert_eq!(w.slug, slug);
        assert_eq!((w.state, w.players), (WorldState::Offline, None));
    }

    /// A world from the list, as the community list gives it: `emulator`, and the id the
    /// launcher's table may know it by.
    fn listed(slug: &str, emulator: Emulator, list_id: Option<&str>) -> World {
        let mut w = World::new(slug, slug);
        w.emulator = emulator;
        w.list_id = list_id.map(str::to_owned);
        crate::known::apply(&mut w);
        w
    }

    /// What the Dereth client is told about the world: the tail of its command line.
    fn told(w: &World) -> Vec<String> {
        crate::launch::describe(w).args()
    }

    /// The era a world plays, and its systems, are the world's own word; else the launcher's
    /// table's, which speaks for the worlds it knows; else the end of retail, which the client is
    /// not told and plays from its later files. The player is never asked.
    #[test]
    fn a_world_plays_the_era_it_says_else_the_tables_else_the_end_of_retail() {
        const SEEDSOW: &str = "394C58D0-885D-466B-B17F-D7E0B96FE3E2";
        let mut state = LauncherState::default();
        let added = state
            .add_custom_world("", "play.example.org", "9000", None, Emulator::Unknown)
            .unwrap();
        let unmarked = all(&[], &state).next().unwrap();
        assert_eq!(unmarked.slug, added);
        let says = |era: Option<&str>, features: Option<&str>| LiveStatus {
            state: WorldState::Online,
            era: era.map(str::to_owned),
            era_features: features.map(str::to_owned),
            software: Some("Empyrean".into()),
            ..LiveStatus::default()
        };

        // 1. What the world says of itself: a server added by hand, unmarked, that answered the
        // status ping with its era and systems.
        let ping = says(Some("infiltration"), Some("trade=false,aetheria=true"));
        let w = shown(unmarked.clone(), None, Some(&ping), None, &state);
        assert_eq!(
            (w.era.as_deref(), w.era_source, w.features_source),
            (Some("infiltration"), Some(Told::World), Some(Told::World))
        );
        assert_eq!(
            told(&w),
            ["--era", "infiltration", "--era-features", "1:0002de"]
        );
        assert_eq!(
            crate::choices::required_set_for(&w),
            crate::datset::SetKind::Classic
        );
        // ... over what the table says of a world it knows.
        let w = shown(
            listed("seedsow", Emulator::Gdle, Some(SEEDSOW)),
            None,
            Some(&says(Some("eor"), None)),
            None,
            &state,
        );
        assert_eq!(
            (w.era.as_deref(), w.era_source),
            (Some("eor"), Some(Told::World))
        );
        assert_eq!(
            (w.era_features.as_deref(), w.features_source),
            (None, None),
            "an era named without its systems has its whole table, not the systems said for another"
        );
        assert!(told(&w).windows(2).any(|p| p == ["--era", "eor"]));
        assert!(!told(&w).contains(&"--era-features".to_owned()));

        // 2. Else the launcher's table, for a world it knows.
        let w = shown(
            listed("seedsow", Emulator::Gdle, Some(SEEDSOW)),
            Some(true),
            None,
            None,
            &state,
        );
        assert_eq!(
            (w.era.as_deref(), w.era_source, w.features_source),
            (Some("infiltration"), Some(Told::World), Some(Told::World))
        );
        assert_eq!(
            told(&w),
            [
                "--world-base",
                "modern",
                "--era",
                "infiltration",
                "--era-features",
                "1:0000df",
                "--logon-version",
                "1802",
                "--world-profile",
                "classicdereth"
            ]
        );

        // 3. Else the end of retail: a plain ACE world, and a server added by hand that does not
        // answer the status ping. No era is named, and the client is told nothing.
        for w in [listed("achard", Emulator::Ace, None), unmarked] {
            let w = shown(w, Some(true), None, None, &state);
            assert_eq!(
                (w.era.as_deref(), w.era_source, w.features_source),
                (None, None, None),
                "{}",
                w.slug
            );
            assert!(told(&w).is_empty(), "{}", w.slug);
            assert_eq!(
                crate::choices::required_set_for(&w),
                crate::datset::SetKind::Modern
            );
        }

        // A world that says its era but not its systems plays that era's whole table.
        let w = shown(
            listed("achard", Emulator::Ace, None),
            None,
            Some(&says(Some("infiltration"), None)),
            None,
            &state,
        );
        assert_eq!(told(&w), ["--era", "infiltration"]);
    }
}
