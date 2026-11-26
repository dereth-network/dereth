//! What a world may be played with: the Dereth client or the retail client, and for the Dereth
//! client, which Modern set and which Classic set. Every front end offers the same choices, so the
//! rules live here rather than in any one of them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use dereth_primitives::EraId;

use crate::datset::{DatOrigin, DatRole, DatSet, SetKind};
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

/// Whether the retail client can play `world`: not one drawn from the 1999 files, whose wire and
/// data it has never spoken, nor a New Dereth world, whose land only the Dereth client takes in.
#[must_use]
pub fn retail_can_play(world: &World) -> bool {
    !plays_launch_pair(world) && !plays_new_dereth(world)
}

/// Whether `world` plays New Dereth: the end of retail's data files with the world's own land,
/// which the world sends the Dereth client when it joins.
#[must_use]
pub fn plays_new_dereth(world: &World) -> bool {
    world.era.as_deref().and_then(EraId::parse) == Some(EraId::NewDereth)
}

/// The base a world names for itself ([`World::world_base`]), in lowercase.
fn named_base(world: &World) -> Option<String> {
    world.world_base.as_deref().map(str::to_ascii_lowercase)
}

/// Whether `world` is drawn from Launch's pair, the 1999 files: its era is Launch, or it names that
/// pair as its base (`original`, as a world written when the pair was a kind of its own says).
#[must_use]
pub fn plays_launch_pair(world: &World) -> bool {
    match named_base(world).as_deref() {
        Some("original") => true,
        Some("modern") => false,
        _ => world.era.as_deref().and_then(EraId::parse) == Some(EraId::Launch),
    }
}

/// Whether the Classic set `set` is a pair `world` may be given: Launch's pair on a world drawn
/// from it, any other year's pair on any other world.
#[must_use]
pub fn pair_suits(world: &World, set: &DatSet) -> bool {
    set.kind.is_pair() && (set.pair_era() == Some(EraId::Launch)) == plays_launch_pair(world)
}

/// The clients there are, Dereth first, each with whether this world accepts it. `dereth` is the
/// Dereth client the launcher found, if it found one. A client that can't play the world isn't
/// offered at all.
pub fn clients_for(
    state: &LauncherState,
    world: &World,
    dereth: Option<&Installation>,
) -> Vec<ClientOption> {
    dereth
        .into_iter()
        .chain(state.retail.as_ref().filter(|_| retail_can_play(world)))
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

/// The Modern sets the Dereth client may be given on this world.
///
/// A world that does not ship its own gets the shared set (and any set the player has not
/// assigned), whether or not it updates its files over the network: the Dereth client keeps a
/// world's updates in that world's own overlay and never writes a set. One that ships its own gets
/// only its own private set, or a custom set matching what it published: never a set another world
/// might be using. A New Dereth world gets only the end of retail's files, which its land is
/// made over.
pub fn dat_sets_for(state: &LauncherState, world: &World) -> Vec<DatSet> {
    // A world that publishes no hash for its own files (the ones the launcher knows by their list
    // id) is matched by what its files report: any set of the player's that reports it, wherever
    // it came from, and none that does not.
    let by_iterations = world
        .dats
        .custom
        .as_ref()
        .filter(|c| c.sha256.is_none())
        .is_some();
    state
        .dat_sets
        .iter()
        .filter(|s| s.kind == SetKind::Modern)
        .filter(|s| match &s.origin {
            DatOrigin::World { slug } => slug == &world.slug,
            _ if by_iterations => set_matches(world, s).is_ok(),
            DatOrigin::Custom { sha256 } => world.dats.custom.as_ref().is_some_and(|c| {
                c.sha256
                    .as_deref()
                    .is_none_or(|h| h.eq_ignore_ascii_case(sha256))
            }),
            DatOrigin::Shared | DatOrigin::Unassigned => world.dats.custom.is_none(),
        })
        .filter(|s| !plays_new_dereth(world) || s.iterations().is_end_of_retail())
        .cloned()
        .collect()
}

/// The files a world compares: those its server checks (GDLE's covers portal and cell; the ACE
/// family's the local file too), and of those only the ones the world says.
fn compared_roles(world: &World) -> &'static [DatRole] {
    if world.emulator == crate::world::Emulator::Gdle {
        &[DatRole::Portal, DatRole::Cell]
    } else {
        &[DatRole::Portal, DatRole::Cell, DatRole::Local]
    }
}

/// Whether `set` holds the files a world that ships its own wants: what each file reports equals
/// what the world's files report, file by file. Any set does for a world that ships none.
///
/// # Errors
/// What is wrong, for the player: which files differ, and where the world's own come from.
pub fn set_matches(world: &World, set: &DatSet) -> Result<(), String> {
    let Some(custom) = &world.dats.custom else {
        return Ok(());
    };
    let have = set.iterations();
    let c = have.compare(&custom.iterations, compared_roles(world));
    if c.matches() {
        return Ok(());
    }
    let mut out = format!(
        "These are not {}'s data files: they read {}, and {}'s read {}.",
        world.name,
        have.label(),
        world.name,
        custom.iterations.label()
    );
    if let Some(note) = &custom.license_note {
        out.push(' ');
        out.push_str(note);
    }
    Err(out)
}

/// The set a world page should start on: the one last used there if it may still be used, else
/// the shared set when it may be, else the first that matches.
pub fn default_set_for(state: &LauncherState, world: &World, last: Option<&str>) -> Option<String> {
    let sets = dat_sets_for(state, world);
    last.filter(|id| sets.iter().any(|s| s.id == *id))
        .map(str::to_owned)
        .or_else(|| {
            sets.iter()
                .find(|s| s.origin == DatOrigin::Shared)
                .or_else(|| sets.first())
                .map(|s| s.id.clone())
        })
}

/// The data set a world's world is drawn from: the one it names ([`World::world_base`]), else its
/// era's ([`crate::eras::required_set`]).
pub fn required_set_for(world: &World) -> SetKind {
    match named_base(world).as_deref() {
        Some("modern") => SetKind::Modern,
        Some("classic" | "original") => SetKind::Classic,
        _ => crate::eras::required_set(world.era.as_deref()),
    }
}

/// [`dat_dirs`] for a world: its base set required, as [`required_set_for`] says.
///
/// # Errors
/// [`MissingSet`] when the required set was not chosen.
pub fn dat_dirs_for(
    world: &World,
    modern: Option<&Path>,
    classic: Option<&Path>,
) -> Result<DatDirs, MissingSet> {
    match required_set_for(world) {
        SetKind::Classic if plays_launch_pair(world) => {
            dat_dirs(Some(EraId::Launch.name()), modern, classic)
        }
        SetKind::Classic => dat_dirs(Some(EraId::Infiltration.name()), modern, classic),
        SetKind::Modern | SetKind::Auberean => dat_dirs(None, modern, classic),
    }
}

/// The pairs the Dereth client may be given on a world (`--classic-dat-dir`): every Classic set
/// whose iterations suit it ([`pair_suits`]), the default first. No world patches or ships them,
/// so any world may read any of them.
pub fn classic_sets_for(state: &LauncherState, world: &World) -> Vec<DatSet> {
    let mut sets: Vec<DatSet> = state
        .dat_sets
        .iter()
        .filter(|s| pair_suits(world, s))
        .cloned()
        .collect();
    sets.sort_by_key(|s| s.origin != DatOrigin::Shared);
    sets
}

/// The folders the Dereth client is started with: `--dat-dir` and, when there is one,
/// `--classic-dat-dir`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatDirs {
    pub dat_dir: PathBuf,
    pub classic_dat_dir: Option<PathBuf>,
}

/// Why a launch's data files do not do for the world's era.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingSet {
    /// No Modern set was chosen. Every world needs one: it is the world's own set for an era
    /// from Throne of Destiny on, and the interface's for an era before it.
    Modern,
    /// The era is before Throne of Destiny, and no Classic set was chosen.
    Classic,
    /// The era is Launch, and no Classic set of Launch's (the 1999 files) was chosen.
    Launch,
}

impl core::fmt::Display for MissingSet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            MissingSet::Modern => "Choose the Modern data files for the Dereth client.",
            MissingSet::Classic => {
                "This world's era is before Throne of Destiny: choose Classic data files (portal.dat and cell.dat) as well as the Modern ones."
            }
            MissingSet::Launch => {
                "This world plays the 1999 launch: choose the Launch data files (the 1999 portal.dat and cell.dat) as well as the Modern ones."
            }
        })
    }
}

/// The folders for a launch, from the world's era and the sets chosen.
///
/// The Modern set is always required. An era before Throne of Destiny also requires the Classic
/// set, which its world is drawn from, with the Modern set beside it for the interface; for any
/// other era (and for no era) the Classic set is optional. The Modern set is `--dat-dir` and the
/// Classic set `--classic-dat-dir`.
///
/// # Errors
/// [`MissingSet`] when the required set was not chosen.
pub fn dat_dirs(
    era: Option<&str>,
    modern: Option<&Path>,
    classic: Option<&Path>,
) -> Result<DatDirs, MissingSet> {
    match crate::eras::required_set(era) {
        SetKind::Classic => {
            let missing = if era.and_then(EraId::parse) == Some(EraId::Launch) {
                MissingSet::Launch
            } else {
                MissingSet::Classic
            };
            let classic = classic.ok_or(missing)?;
            let dat_dir = modern.ok_or(MissingSet::Modern)?.to_path_buf();
            Ok(DatDirs {
                classic_dat_dir: (dat_dir != classic).then(|| classic.to_path_buf()),
                dat_dir,
            })
        }
        SetKind::Modern | SetKind::Auberean => {
            let dat_dir = modern.ok_or(MissingSet::Modern)?.to_path_buf();
            Ok(DatDirs {
                dat_dir,
                classic_dat_dir: classic.map(Path::to_path_buf),
            })
        }
    }
}

/// Whether playing an Asheron's Call 2 world with the retail client first makes the world a private
/// copy of the retail client's install, which it then plays from: the world writes into the
/// install ([`World::writes_retail_install`]), there is a retail client to copy, and the world has
/// no copy yet. The player's own install is never written.
pub fn copies_retail_at_play(state: &LauncherState, world: &World) -> bool {
    world.game == crate::world::Game::Auberean
        && world.writes_retail_install()
        && state.auberean.retail.is_some()
        && !state.auberean.retail_copies.contains_key(&world.slug)
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
    fn every_world_needs_the_modern_set_and_an_era_before_throne_of_destiny_the_classic_one_too() {
        let (m, c) = (Path::new("/lib/modern"), Path::new("/lib/classic"));
        // The end of retail, or no era: the Modern set, with the Classic one beside it if chosen.
        for era in [None, Some("eor")] {
            assert_eq!(
                dat_dirs(era, Some(m), None),
                Ok(DatDirs {
                    dat_dir: m.into(),
                    classic_dat_dir: None
                })
            );
            assert_eq!(
                dat_dirs(era, Some(m), Some(c)),
                Ok(DatDirs {
                    dat_dir: m.into(),
                    classic_dat_dir: Some(c.into())
                })
            );
            assert_eq!(dat_dirs(era, None, Some(c)), Err(MissingSet::Modern));
        }
        // Before Throne of Destiny: the Classic set is required.
        let pre = Some("infiltration");
        assert_eq!(dat_dirs(pre, Some(m), None), Err(MissingSet::Classic));
        assert_eq!(
            dat_dirs(pre, Some(m), Some(c)),
            Ok(DatDirs {
                dat_dir: m.into(),
                classic_dat_dir: Some(c.into())
            })
        );
        assert_eq!(
            dat_dirs(pre, None, Some(c)),
            Err(MissingSet::Modern),
            "the Classic set alone does not start: the interface reads the later files"
        );
        assert_eq!(dat_dirs(pre, None, None), Err(MissingSet::Classic));
        // One folder holding both kinds is named once.
        assert_eq!(
            dat_dirs(pre, Some(m), Some(m)),
            Ok(DatDirs {
                dat_dir: m.into(),
                classic_dat_dir: None
            })
        );
    }

    #[test]
    fn classic_sets_are_offered_by_their_iterations_the_1999_pair_to_launch_and_others_elsewhere() {
        let pair = |id: &str, portal: u32, cell: u32, origin| {
            let d = crate::datset::tests::tmp(&format!("choices-pair-{id}"));
            crate::testing::write_classic_set(&d, portal, cell);
            DatSet {
                id: id.into(),
                path: d.clone(),
                kind: SetKind::Classic,
                origin,
                files: crate::datset::scan_classic_dir(&d),
                last_patched_by_server: None,
                created_by_launcher: false,
            }
        };
        let modern = DatSet {
            id: "m".into(),
            path: PathBuf::from("m"),
            kind: SetKind::Modern,
            origin: DatOrigin::Shared,
            files: vec![],
            last_patched_by_server: None,
            created_by_launcher: false,
        };
        let s = LauncherState {
            dat_sets: vec![
                modern,
                pair("aug", 124, 108, DatOrigin::Unassigned),
                pair("feb", 2112, 1593, DatOrigin::Shared),
                pair("nov", 105, 67, DatOrigin::Unassigned),
                pair("cd", 102, 67, DatOrigin::Unassigned),
            ],
            ..Default::default()
        };
        let mut w = World::new("eulmore", "Eulmore");
        let ids = |v: Vec<DatSet>| v.into_iter().map(|d| d.id).collect::<Vec<_>>();
        assert_eq!(ids(dat_sets_for(&s, &w)), ["m"]);
        assert_eq!(
            ids(classic_sets_for(&s, &w)),
            ["feb", "aug"],
            "the default first"
        );
        w.era = Some("infiltration".into());
        assert_eq!(ids(classic_sets_for(&s, &w)), ["feb", "aug"]);
        w.era = Some("launch".into());
        assert_eq!(ids(dat_sets_for(&s, &w)), ["m"]);
        assert_eq!(ids(classic_sets_for(&s, &w)), ["nov", "cd"]);
        // A world written when the 1999 pair was a kind of its own names it as its base.
        w.era = None;
        w.world_base = Some("original".into());
        assert_eq!(ids(classic_sets_for(&s, &w)), ["nov", "cd"]);
        assert_eq!(required_set_for(&w), SetKind::Classic);
        for d in s.dat_sets.iter().skip(1) {
            let _ = std::fs::remove_dir_all(&d.path);
        }
    }

    #[test]
    fn a_launch_world_needs_the_launch_pair_beside_the_modern_one() {
        let (m, o) = (Path::new("/m"), Path::new("/o"));
        let mut w = World::new("x", "X");
        w.era = Some("launch".into());
        assert_eq!(required_set_for(&w), SetKind::Classic);
        assert!(plays_launch_pair(&w));
        assert_eq!(dat_dirs_for(&w, Some(m), None), Err(MissingSet::Launch));
        assert!(MissingSet::Launch.to_string().contains("Launch data files"));
        assert_eq!(
            dat_dirs_for(&w, Some(m), Some(o)),
            Ok(DatDirs {
                dat_dir: m.into(),
                classic_dat_dir: Some(o.into())
            })
        );
        assert_eq!(dat_dirs_for(&w, None, Some(o)), Err(MissingSet::Modern));
    }

    /// A set whose files report `it`, with the given origin.
    fn set_reading(id: &str, origin: DatOrigin, it: Iterations) -> DatSet {
        let d = crate::datset::tests::tmp(&format!("choices-{id}"));
        crate::datset::tests::fake_set(&d, it);
        DatSet {
            id: id.into(),
            path: d.clone(),
            kind: SetKind::Modern,
            origin,
            files: crate::datset::scan_dir(&d),
            last_patched_by_server: None,
            created_by_launcher: false,
        }
    }

    fn known_world(list_id: &str, name: &str) -> World {
        let mut w = World::new(crate::serverlist::slug_for(name), name);
        w.list_id = Some(list_id.into());
        crate::known::apply(&mut w);
        w
    }

    #[test]
    fn a_known_world_is_offered_only_the_sets_whose_files_report_its_own() {
        let customdm = Iterations {
            portal: Some(20044),
            cell: Some(20011),
            local: Some(20011),
            highres: Some(497),
        };
        let classicdereth = Iterations {
            portal: Some(2072),
            cell: Some(4),
            local: Some(994),
            highres: Some(497),
        };
        let s = LauncherState {
            dat_sets: vec![
                set_reading("eor", DatOrigin::Shared, Iterations::END_OF_RETAIL),
                set_reading("cdm", DatOrigin::Unassigned, customdm),
                set_reading("cd", DatOrigin::Unassigned, classicdereth),
            ],
            ..Default::default()
        };
        let ids = |v: Vec<DatSet>| v.into_iter().map(|d| d.id).collect::<Vec<_>>();
        let dekarutide = known_world("4db379b0-587a-4775-9b7f-879e0f15ba4b", "Dekarutide");
        assert_eq!(ids(dat_sets_for(&s, &dekarutide)), ["cdm"]);
        assert_eq!(
            default_set_for(&s, &dekarutide, Some("eor")).as_deref(),
            Some("cdm"),
            "a set that does not match is not kept as the choice"
        );
        // GDLE compares portal and cell only, so the local file's count does not matter.
        let seedsow = known_world("394C58D0-885D-466B-B17F-D7E0B96FE3E2", "Seedsow");
        assert_eq!(ids(dat_sets_for(&s, &seedsow)), ["cd"]);
        // An ordinary world is offered the shared set and the unassigned ones, as before.
        assert_eq!(
            ids(dat_sets_for(&s, &World::new("coldeve", "Coldeve"))),
            ["eor", "cdm", "cd"]
        );
        for d in &s.dat_sets {
            let _ = std::fs::remove_dir_all(&d.path);
        }
    }

    #[test]
    fn a_known_world_with_none_of_its_files_is_offered_nothing_and_a_wrong_set_is_refused() {
        let s = LauncherState {
            dat_sets: vec![set_reading(
                "eor2",
                DatOrigin::Shared,
                Iterations::END_OF_RETAIL,
            )],
            ..Default::default()
        };
        let shores = known_world("3f1f41ec-c7fd-4ed9-b47d-25b0d94219c1", "Unfamiliar Shores");
        assert!(dat_sets_for(&s, &shores).is_empty());
        assert_eq!(default_set_for(&s, &shores, None), None);
        let refusal = set_matches(&shores, &s.dat_sets[0]).unwrap_err();
        assert!(
            refusal.contains("2072/982/994/497")
                && refusal.contains("20044/20011/20011/497")
                && refusal.contains("CustomDM"),
            "{refusal}"
        );
        assert_eq!(set_matches(&World::new("a", "A"), &s.dat_sets[0]), Ok(()));
        let _ = std::fs::remove_dir_all(&s.dat_sets[0].path);
    }

    #[test]
    fn a_known_world_drawn_from_the_later_files_needs_the_modern_set_whatever_its_era() {
        let w = known_world("4db379b0-587a-4775-9b7f-879e0f15ba4b", "Dekarutide");
        assert_eq!(w.era.as_deref(), Some("infiltration"));
        assert_eq!(required_set_for(&w), SetKind::Modern);
        assert_eq!(
            dat_dirs_for(&w, None, Some(Path::new("/c"))),
            Err(MissingSet::Modern)
        );
        assert_eq!(
            dat_dirs_for(&w, Some(Path::new("/m")), Some(Path::new("/c"))),
            Ok(DatDirs {
                dat_dir: "/m".into(),
                classic_dat_dir: Some("/c".into())
            })
        );
        let mut plain = World::new("x", "X");
        plain.era = Some("infiltration".into());
        assert_eq!(required_set_for(&plain), SetKind::Classic);
    }

    /// The retail client can't play the 1999 wire or files: a Launch world, hosted, listed or through
    /// a relay, offers Dereth alone, and the last choice there doesn't bring it back.
    #[test]
    fn a_launch_world_offers_dereth_alone() {
        let s = LauncherState {
            retail: Some(install(ClientKind::Retail, "retail")),
            ..LauncherState::default()
        };
        let dereth = install(ClientKind::Dereth, "dereth");
        let mut w = World::new("eulmore", "Eulmore");
        assert_eq!(clients_for(&s, &w, Some(&dereth)).len(), 2, "any other era");

        w.era = Some("launch".into());
        let opts = clients_for(&s, &w, Some(&dereth));
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].kind, ClientKind::Dereth);
        assert_eq!(
            default_client(&opts, &w, Some(ClientKind::Retail)),
            Some(ClientKind::Dereth)
        );

        let relayed = crate::relayed::RELAYED_WORLDS[0].to_world();
        assert!(!retail_can_play(&relayed));
    }

    /// A New Dereth world is played with the Dereth client alone, which takes in the land the
    /// world sends, on the end of retail's files: the shared set first, never a set of other
    /// files, and a last choice of the retail client doesn't bring it back.
    #[test]
    fn a_new_dereth_world_plays_the_dereth_client_on_the_end_of_retails_files() {
        let older = Iterations {
            portal: Some(2000),
            cell: Some(900),
            local: Some(994),
            highres: Some(497),
        };
        let s = LauncherState {
            retail: Some(install(ClientKind::Retail, "acclient-6096")),
            dat_sets: vec![
                set_reading("nd-other", DatOrigin::Unassigned, older),
                set_reading("nd-spare", DatOrigin::Unassigned, Iterations::END_OF_RETAIL),
                set_reading("nd-eor", DatOrigin::Shared, Iterations::END_OF_RETAIL),
            ],
            ..LauncherState::default()
        };
        let dereth = install(ClientKind::Dereth, "dereth");
        let mut w = World::new("arwic", "Arwic");
        w.era = Some("newdereth".into());
        assert!(plays_new_dereth(&w) && !retail_can_play(&w));
        let opts = clients_for(&s, &w, Some(&dereth));
        assert_eq!(
            opts.iter().map(|o| o.kind).collect::<Vec<_>>(),
            [ClientKind::Dereth]
        );
        assert_eq!(
            default_client(&opts, &w, Some(ClientKind::Retail)),
            Some(ClientKind::Dereth)
        );
        let ids = |v: Vec<DatSet>| v.into_iter().map(|d| d.id).collect::<Vec<_>>();
        assert_eq!(ids(dat_sets_for(&s, &w)), ["nd-spare", "nd-eor"]);
        assert_eq!(default_set_for(&s, &w, None).as_deref(), Some("nd-eor"));
        assert_eq!(
            default_set_for(&s, &w, Some("nd-other")).as_deref(),
            Some("nd-eor"),
            "a set of other files is not kept as the choice"
        );
        assert_eq!(required_set_for(&w), SetKind::Modern);
        // Any other era keeps the retail client and every set it had.
        w.era = Some("eor".into());
        assert!(retail_can_play(&w));
        assert_eq!(ids(dat_sets_for(&s, &w)).len(), 3);
        for d in &s.dat_sets {
            let _ = std::fs::remove_dir_all(&d.path);
        }
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

    /// Playing an Asheron's Call 2 world that patches or serves the admin library with the retail
    /// client makes it a private copy of the retail install, once there is a retail client and
    /// until the world has a copy; a world that writes nothing, or one of Asheron's Call, doesn't.
    #[test]
    fn a_world_that_writes_into_the_retail_install_is_copied_at_play_until_it_has_a_copy() {
        let mut state = LauncherState::default();
        let mut world = World {
            slug: "dawnsong".into(),
            game: crate::world::Game::Auberean,
            ..World::default()
        };
        world.dats.admin_library = Some(true);
        assert!(
            !copies_retail_at_play(&state, &world),
            "no retail client to copy"
        );
        state.auberean.retail = Some(install(ClientKind::Retail, "ac2client-0930"));
        assert!(copies_retail_at_play(&state, &world));
        world.dats.admin_library = None;
        assert!(!copies_retail_at_play(&state, &world), "writes nothing");
        world.dats.patches_over_wire = Some(true);
        assert!(copies_retail_at_play(&state, &world));
        state
            .auberean
            .retail_copies
            .insert("dawnsong".into(), PathBuf::from("C:/copies/dawnsong"));
        assert!(!copies_retail_at_play(&state, &world), "it has one");
        world.slug = "other".into();
        assert!(copies_retail_at_play(&state, &world));
        world.game = crate::world::Game::Dereth;
        assert!(
            !copies_retail_at_play(&state, &world),
            "a world of the other game"
        );
    }
}
