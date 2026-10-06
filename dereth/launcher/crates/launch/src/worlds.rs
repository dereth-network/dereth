//! The worlds a launcher shows: the community list's, then the servers the player added, each as
//! it last said of itself (its status) and with what the player chose for what it does not say.
//!
//! Every front end over this crate (the launcher's window, a web page) shows a world this way, so
//! a world reads the same in each.

use crate::state::LauncherState;
use crate::status::LiveStatus;
use crate::world::{AccountModel, Emulator, Told, World, WorldState};

/// Every world: the list's, then the servers the player added, as their rows say.
pub fn all<'a>(list: &'a [World], state: &'a LauncherState) -> impl Iterator<Item = World> + 'a {
    list.iter()
        .cloned()
        .chain(state.custom_worlds.iter().map(|c| c.to_world()))
}

/// `w` as it is shown: up or down by whether it answered a probe (`probed`), what its status says
/// (`live`: its state, players, software, era, systems and files), and, for what neither says, the
/// era and systems the player chose for it.
#[must_use]
pub fn shown(
    mut w: World,
    probed: Option<bool>,
    live: Option<&LiveStatus>,
    state: &LauncherState,
) -> World {
    // A world with no status document is up or down by whether it answered the probe.
    if let Some(up) = probed {
        w.state = if up {
            WorldState::Online
        } else {
            WorldState::Offline
        };
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
        if l.era.is_some() {
            w.era.clone_from(&l.era);
            w.era_source = Some(Told::World);
        }
        if l.era_features.is_some() {
            w.era_features.clone_from(&l.era_features);
            w.features_source = Some(Told::World);
        }
        if w.account_model == AccountModel::Unknown && l.auto_create_accounts == Some(true) {
            w.account_model = AccountModel::AutoCreateOnFirstLogin;
        }
    }
    // What the world does not say, the player may have chosen.
    if let Some(c) = state.world_eras.get(&w.slug) {
        if w.era.is_none() && c.era.is_some() {
            w.era.clone_from(&c.era);
            w.era_source = Some(Told::Player);
        }
        if w.era_features.is_none() {
            if let Some(text) = c.features_text() {
                w.era_features = Some(text);
                w.features_source = Some(Told::Player);
            }
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Emulator;

    #[test]
    fn a_world_is_shown_with_its_status_over_its_row_and_the_players_choice_under_both() {
        let mut state = LauncherState::default();
        let slug = state
            .add_custom_world("", "play.example.org", "9000", None, Emulator::Unknown)
            .unwrap();
        state.set_world_era(&slug, Some("infiltration"));
        let listed = World::new("coldeve", "Coldeve");
        let worlds: Vec<World> = all(std::slice::from_ref(&listed), &state).collect();
        assert_eq!(
            worlds.iter().map(|w| w.slug.as_str()).collect::<Vec<_>>(),
            ["coldeve", slug.as_str()],
            "the list's, then the player's"
        );

        // Nothing said: the player's era.
        let w = shown(worlds[1].clone(), None, None, &state);
        assert_eq!(
            (w.era.as_deref(), w.era_source),
            (Some("infiltration"), Some(Told::Player))
        );
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
        let w = shown(worlds[1].clone(), None, Some(&live), &state);
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
        let w = shown(listed, Some(false), None, &state);
        assert_eq!(w.state, WorldState::Offline);
    }
}
