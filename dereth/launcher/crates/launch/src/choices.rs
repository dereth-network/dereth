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

/// The Modern sets the Dereth client may be given on this world.
///
/// A world that neither patches nor ships its own gets the shared set (and any set the player has
/// not assigned). One that does gets only its own private set, or a custom set matching what it
/// published: never a set another world might be using.
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
            DatOrigin::Shared | DatOrigin::Unassigned => !world.needs_private_dats(),
        })
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
/// the first that matches.
pub fn default_set_for(state: &LauncherState, world: &World, last: Option<&str>) -> Option<String> {
    let sets = dat_sets_for(state, world);
    last.filter(|id| sets.iter().any(|s| s.id == *id))
        .map(str::to_owned)
        .or_else(|| sets.first().map(|s| s.id.clone()))
}

/// The data set a world's world is drawn from: the one it names ([`World::world_base`]), else its
/// era's ([`crate::eras::required_set`]).
pub fn required_set_for(world: &World) -> SetKind {
    match world
        .world_base
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("modern") => SetKind::Modern,
        Some("classic") => SetKind::Classic,
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
        SetKind::Modern => dat_dirs(None, modern, classic),
        SetKind::Classic => dat_dirs(Some(EraId::Infiltration.name()), modern, classic),
    }
}

/// The Classic sets the Dereth client may be given: every one. No world patches or ships them, so
/// any world may read any of them.
pub fn classic_sets_for(state: &LauncherState) -> Vec<DatSet> {
    state
        .dat_sets
        .iter()
        .filter(|s| s.kind == SetKind::Classic)
        .cloned()
        .collect()
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
    /// The era's world is drawn from the Modern set, and none was chosen.
    Modern,
    /// The era is before Throne of Destiny, and no Classic set was chosen.
    Classic,
}

impl core::fmt::Display for MissingSet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            MissingSet::Modern => "Choose the Modern data files for the Dereth client.",
            MissingSet::Classic => {
                "This world's era is before Throne of Destiny: choose Classic data files (portal.dat and cell.dat)."
            }
        })
    }
}

/// The folders for a launch, from the world's era and the sets chosen.
///
/// The era decides which set is required: the Classic set for an era before Throne of Destiny,
/// the Modern set for any other (and for no era). The other is optional. The Modern set is
/// `--dat-dir` and the Classic set `--classic-dat-dir`; a Classic-era world with no Modern set
/// chosen is started with the Classic set as `--dat-dir`, which the client then plays alone.
///
/// # Errors
/// [`MissingSet`] when the required set was not chosen.
pub fn dat_dirs(
    era: Option<&str>,
    modern: Option<&Path>,
    classic: Option<&Path>,
) -> Result<DatDirs, MissingSet> {
    match crate::eras::required_set(era) {
        SetKind::Modern => {
            let dat_dir = modern.ok_or(MissingSet::Modern)?.to_path_buf();
            Ok(DatDirs {
                dat_dir,
                classic_dat_dir: classic.map(Path::to_path_buf),
            })
        }
        SetKind::Classic => {
            let classic = classic.ok_or(MissingSet::Classic)?;
            let dat_dir = modern.unwrap_or(classic).to_path_buf();
            Ok(DatDirs {
                classic_dat_dir: (dat_dir != classic).then(|| classic.to_path_buf()),
                dat_dir,
            })
        }
    }
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
    fn the_era_decides_which_set_is_required_and_the_other_is_optional() {
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
            Ok(DatDirs {
                dat_dir: c.into(),
                classic_dat_dir: None
            }),
            "the Classic set alone is the data folder"
        );
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
    fn classic_sets_are_offered_to_every_world_and_never_as_modern_ones() {
        let set = |id: &str, kind| DatSet {
            id: id.into(),
            path: PathBuf::from(id),
            kind,
            origin: DatOrigin::Unassigned,
            files: vec![],
            last_patched_by_server: None,
            created_by_launcher: false,
        };
        let s = LauncherState {
            dat_sets: vec![set("m", SetKind::Modern), set("c", SetKind::Classic)],
            ..Default::default()
        };
        let w = World::new("eulmore", "Eulmore");
        let ids = |v: Vec<DatSet>| v.into_iter().map(|d| d.id).collect::<Vec<_>>();
        assert_eq!(ids(dat_sets_for(&s, &w)), ["m"]);
        assert_eq!(ids(classic_sets_for(&s)), ["c"]);
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
