//! What a world may be played with: the Dereth client or the retail client, and for the Dereth
//! client, which dat set. Every front end offers the same choices, so the rules live here rather
//! than in any one of them.

use serde::{Deserialize, Serialize};

use crate::datset::{DatOrigin, DatSet};
use crate::install::{ClientKind, Installation};
use crate::state::LauncherState;
use crate::world::World;

/// One client a world page can offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientOption {
    pub kind: ClientKind,
    pub install: Installation,
    /// The world accepts this client.
    pub accepted: bool,
}

/// The clients there are, Dereth first, each with whether this world accepts it. `dereth` is the
/// Dereth client the launcher found, if it found one.
pub fn clients_for(
    state: &LauncherState,
    world: &World,
    dereth: Option<&Installation>,
) -> Vec<ClientOption> {
    dereth
        .into_iter()
        .chain(state.retail.as_ref())
        .map(|i| ClientOption {
            kind: i.kind,
            install: i.clone(),
            accepted: world.accepts(&i.client_id, i.net_version.as_deref()),
        })
        .collect()
}

/// The client a world page should start on: the one last used there if it is still offered, else
/// the world's preferred one, else the first accepted one, else anything.
pub fn default_client(
    options: &[ClientOption],
    world: &World,
    last: Option<ClientKind>,
) -> Option<ClientKind> {
    let offered = |k: ClientKind| options.iter().any(|o| o.kind == k);
    let preferred = world.preferred_client.as_deref().map(|p| {
        if p == "dereth" || p.starts_with("dereth-") {
            ClientKind::Dereth
        } else {
            ClientKind::Retail
        }
    });
    last.filter(|k| offered(*k))
        .or_else(|| preferred.filter(|k| options.iter().any(|o| o.kind == *k && o.accepted)))
        .or_else(|| options.iter().find(|o| o.accepted).map(|o| o.kind))
        .or_else(|| options.first().map(|o| o.kind))
}

/// The dat sets the Dereth client may be given on this world.
///
/// A world that neither patches nor ships its own gets the shared set (and any set the player has
/// not assigned). One that does gets only its own private set, or a custom set matching what it
/// published: never a set another world might be using.
pub fn dat_sets_for(state: &LauncherState, world: &World) -> Vec<DatSet> {
    state
        .dat_sets
        .iter()
        .filter(|s| match &s.origin {
            DatOrigin::World { slug } => slug == &world.slug,
            DatOrigin::Custom { sha256 } => world.dats.custom.as_ref().is_some_and(|c| {
                c.sha256
                    .as_deref()
                    .is_none_or(|h| h.eq_ignore_ascii_case(sha256))
            }),
            DatOrigin::Shared | DatOrigin::Unassigned => !world.needs_private_dats(),
        })
        .cloned()
        .collect()
}

/// Whether to offer "Create a private copy": the world needs a set of its own, has none, and is not
/// one whose set comes from its own download.
pub fn offers_private_copy(state: &LauncherState, world: &World) -> bool {
    world.needs_private_dats()
        && world.dats.custom.is_none()
        && state.private_set_for(&world.slug).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datset::Iterations;
    use crate::install::IdentifiedBy;
    use std::path::PathBuf;

    fn install(kind: ClientKind, client_id: &str) -> Installation {
        Installation {
            id: "x".into(),
            path: PathBuf::from("C:/x"),
            exe: "x.exe".into(),
            kind,
            client_id: client_id.into(),
            version: None,
            build_date: None,
            net_version: Some("1802".into()),
            identified_by: IdentifiedBy::ExeSha256,
            modifications: vec![],
            multi_instance: true,
            own_dats: Iterations::default(),
            verified_at: None,
            manifest_result: None,
        }
    }

    #[test]
    fn the_dereth_client_comes_first_and_the_last_choice_wins() {
        let s = LauncherState {
            retail: Some(install(ClientKind::Retail, "acclient-6096")),
            ..Default::default()
        };
        let dereth = install(ClientKind::Dereth, "dereth");
        let w = World::new("eulmore", "Eulmore");
        let opts = clients_for(&s, &w, Some(&dereth));
        assert_eq!(
            opts.iter().map(|o| o.kind).collect::<Vec<_>>(),
            [ClientKind::Dereth, ClientKind::Retail]
        );
        assert!(opts.iter().all(|o| o.accepted));
        assert_eq!(default_client(&opts, &w, None), Some(ClientKind::Dereth));
        assert_eq!(
            default_client(&opts, &w, Some(ClientKind::Retail)),
            Some(ClientKind::Retail)
        );
    }

    #[test]
    fn without_a_retail_client_only_dereth_is_offered() {
        let s = LauncherState::default();
        let w = World::new("eulmore", "Eulmore");
        let opts = clients_for(&s, &w, Some(&install(ClientKind::Dereth, "dereth")));
        assert_eq!(opts.len(), 1);
        assert_eq!(
            default_client(&opts, &w, Some(ClientKind::Retail)),
            Some(ClientKind::Dereth)
        );
        assert_eq!(default_client(&[], &w, None), None);
    }
}
