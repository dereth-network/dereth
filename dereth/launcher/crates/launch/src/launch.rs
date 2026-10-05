//! Command lines: what to run, from where, and with which arguments.
//!
//! A [`LaunchPlan`] never holds the password. It holds a placeholder where the password goes, so
//! the plan can be logged, shown and tested freely, and the secret is put in only at the moment of
//! spawning ([`LaunchPlan::argv`]). The redacted form ([`LaunchPlan::redacted`]) is the only form
//! that may reach a log.
//!
//! The forms, per client and server:
//!
//! | client | form |
//! |---|---|
//! | Dereth | `dereth-client.exe -a <account> -v <password> -h <host> -p <port> --dat-dir <dir> [--classic-dat-dir <dir>] [--era <era>] [--era-features <version>:<bits>]` |
//! | retail, ACE or Empyrean | `acclient.exe -a <account> -v <password> -h <host>:<port>` |
//! | retail, GDLE | `acclient.exe -h <host> -p <port> -a <account>:<password>` |
//!
//! A retail client runs from its own folder and reads the dats beside it. The Dereth client runs
//! from its own folder too, and reads the dat set it is given, and the Classic set when it is
//! given one ([`crate::choices::dat_dirs`] decides which). When the world names the era it
//! plays, the Dereth client is told it, and the systems the world has with it, so its screens show
//! that world's systems from the start. The systems go as the shared bitfield
//! (`dereth_primitives::EraFeatureBits`): the world's whole set, the era's table with what the
//! world or the player turned on or off over it.

use std::path::PathBuf;

use dereth_primitives::{EraFeatureBits, EraFeatureOverrides, EraId};

use crate::install::{ClientKind, Installation};
use crate::world::{Emulator, World};

/// The world's whole set of systems as the bitfield the Dereth client reads: its era's table (the
/// end of retail's when it names none) with [`World::era_features`] over it. `None` when nobody
/// said its systems, or what was said does not read.
fn era_feature_bits(world: &World) -> Option<EraFeatureBits> {
    let text = world
        .era_features
        .as_deref()
        .filter(|f| !f.trim().is_empty())?;
    let (overrides, _unknown) = EraFeatureOverrides::parse(text).ok()?;
    let era = world
        .era
        .as_deref()
        .and_then(EraId::parse)
        .unwrap_or_default();
    Some(EraFeatureBits::of(overrides.apply(era.features())))
}

/// One argument, or a placeholder for a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    Plain(String),
    /// The password, alone.
    Password,
    /// `<account>:<password>`, GDLE's form.
    AccountAndPassword(String),
}

/// Everything needed to start a client, minus the password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub exe: PathBuf,
    pub working_dir: PathBuf,
    pub args: Vec<Arg>,
}

/// What the plan is built from.
#[derive(Debug, Clone)]
pub struct LaunchRequest<'a> {
    pub world: &'a World,
    pub install: &'a Installation,
    pub account: &'a str,
    /// The dats the Dereth client should open. Ignored for a retail client, which reads the dats
    /// beside it.
    pub dat_dir: Option<PathBuf>,
    /// The Classic set (`portal.dat`, `cell.dat`) for the Dereth client, when it is not in
    /// `dat_dir`. Ignored for a retail client.
    pub classic_dat_dir: Option<PathBuf>,
    /// Operator-supplied arguments, placed first.
    pub extra_args: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// The world lists no address to connect to.
    NoEndpoint,
    /// The Dereth client was given no dat set.
    NoDats,
}

impl core::fmt::Display for PlanError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PlanError::NoEndpoint => write!(f, "this world has no address to connect to"),
            PlanError::NoDats => write!(f, "choose the data files for the Dereth client"),
        }
    }
}

impl std::error::Error for PlanError {}

fn plain(s: impl Into<String>) -> Arg {
    Arg::Plain(s.into())
}

/// Build the command line for a launch.
pub fn plan(req: &LaunchRequest<'_>) -> Result<LaunchPlan, PlanError> {
    let ep = req.world.endpoint.as_ref().ok_or(PlanError::NoEndpoint)?;
    let mut args: Vec<Arg> = req.extra_args.iter().cloned().map(Arg::Plain).collect();
    let working_dir = req.install.path.clone();
    let exe = req.install.exe_path();

    match req.install.kind {
        ClientKind::Dereth => {
            let dats = req.dat_dir.as_ref().ok_or(PlanError::NoDats)?;
            args.extend([
                plain("-a"),
                plain(req.account),
                plain("-v"),
                Arg::Password,
                plain("-h"),
                plain(ep.address.clone()),
                plain("-p"),
                plain(ep.port.to_string()),
                plain("--dat-dir"),
                plain(dats.display().to_string()),
            ]);
            if let Some(classic) = &req.classic_dat_dir {
                args.extend([
                    plain("--classic-dat-dir"),
                    plain(classic.display().to_string()),
                ]);
            }
            if let Some(era) = req.world.era.as_deref().filter(|e| !e.is_empty()) {
                args.extend([plain("--era"), plain(era)]);
            }
            if let Some(bits) = era_feature_bits(req.world) {
                args.extend([plain("--era-features"), plain(bits.to_string())]);
            }
        }
        ClientKind::Retail if req.world.emulator == Emulator::Gdle => {
            args.extend([
                plain("-h"),
                plain(ep.address.clone()),
                plain("-p"),
                plain(ep.port.to_string()),
                plain("-a"),
                Arg::AccountAndPassword(req.account.to_owned()),
            ]);
        }
        ClientKind::Retail => {
            args.extend([
                plain("-a"),
                plain(req.account),
                plain("-v"),
                Arg::Password,
                plain("-h"),
                plain(format!("{}:{}", ep.address, ep.port)),
            ]);
        }
    }
    Ok(LaunchPlan {
        exe,
        working_dir,
        args,
    })
}

impl LaunchPlan {
    /// The real argument vector, with the password in place. For spawning only.
    pub fn argv(&self, password: &str) -> Vec<String> {
        self.args
            .iter()
            .map(|a| match a {
                Arg::Plain(s) => s.clone(),
                Arg::Password => password.to_owned(),
                Arg::AccountAndPassword(acct) => format!("{acct}:{password}"),
            })
            .collect()
    }

    /// The command line with every secret replaced by `***`. The only form that may be logged.
    pub fn redacted(&self) -> String {
        let mut out = quote(&self.exe.display().to_string());
        for a in &self.args {
            out.push(' ');
            out.push_str(&match a {
                Arg::Plain(s) => quote(s),
                Arg::Password => "***".into(),
                Arg::AccountAndPassword(acct) => format!("{}:***", quote(acct)),
            });
        }
        out
    }
}

fn quote(s: &str) -> String {
    if s.is_empty() || s.contains([' ', '\t', '"']) {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datset::Iterations;
    use crate::install::IdentifiedBy;
    use crate::world::Endpoint;

    fn world(emu: Emulator) -> World {
        let mut w = World::new("eulmore", "Eulmore");
        w.emulator = emu;
        w.endpoint = Some(Endpoint {
            address: "eulmore.example".into(),
            port: 19000,
            transport: None,
        });
        w
    }

    fn inst(kind: ClientKind) -> Installation {
        Installation {
            id: "i".into(),
            path: PathBuf::from("/games/ac"),
            exe: if kind == ClientKind::Dereth {
                "dereth-client.exe".into()
            } else {
                "acclient.exe".into()
            },
            kind,
            client_id: "x".into(),
            version: None,
            build_date: None,
            net_version: None,
            identified_by: IdentifiedBy::ExeSha256,
            modifications: vec![],
            multi_instance: true,
            own_dats: Iterations::default(),
            verified_at: None,
            manifest_result: None,
        }
    }

    fn req<'a>(w: &'a World, i: &'a Installation) -> LaunchRequest<'a> {
        LaunchRequest {
            world: w,
            install: i,
            account: "player",
            dat_dir: Some(PathBuf::from("/lib/eor")),
            classic_dat_dir: None,
            extra_args: &[],
        }
    }

    #[test]
    fn the_dereth_client_gets_the_account_the_password_the_server_and_its_dat_set() {
        let (w, i) = (world(Emulator::Empyrean), inst(ClientKind::Dereth));
        let p = plan(&req(&w, &i)).unwrap();
        assert_eq!(
            p.argv("hunter2"),
            [
                "-a",
                "player",
                "-v",
                "hunter2",
                "-h",
                "eulmore.example",
                "-p",
                "19000",
                "--dat-dir",
                "/lib/eor"
            ]
        );
        assert_eq!(p.working_dir, PathBuf::from("/games/ac"));
        let mut r = req(&w, &i);
        r.dat_dir = None;
        assert_eq!(plan(&r), Err(PlanError::NoDats));
    }

    #[test]
    fn the_dereth_client_is_told_the_era_the_world_plays() {
        let (mut w, i) = (world(Emulator::Empyrean), inst(ClientKind::Dereth));
        w.era = Some("infiltration".into());
        let argv = plan(&req(&w, &i)).unwrap().argv("pw");
        assert_eq!(argv[argv.len() - 2..], ["--era", "infiltration"]);
        // And the systems its status lists, as the whole set over the era's table: Infiltration's
        // `1:0000df` with trade off and aetheria on.
        w.era_features = Some("trade=false,aetheria=true".into());
        let argv = plan(&req(&w, &i)).unwrap().argv("pw");
        assert_eq!(
            argv[argv.len() - 4..],
            ["--era", "infiltration", "--era-features", "1:0002de"]
        );

        // And the Classic set, between the data folder and the era.
        let mut r = req(&w, &i);
        r.classic_dat_dir = Some(PathBuf::from("/lib/feb2005"));
        let argv = plan(&r).unwrap().argv("pw");
        assert_eq!(
            argv[9..],
            [
                "/lib/eor",
                "--classic-dat-dir",
                "/lib/feb2005",
                "--era",
                "infiltration",
                "--era-features",
                "1:0002de"
            ]
        );

        // A retail client has no such switches.
        let i = inst(ClientKind::Retail);
        let mut r = req(&w, &i);
        r.classic_dat_dir = Some(PathBuf::from("/lib/feb2005"));
        assert_eq!(
            plan(&r).unwrap().argv("pw"),
            ["-a", "player", "-v", "pw", "-h", "eulmore.example:19000"]
        );
        assert!(!plan(&req(&w, &i))
            .unwrap()
            .argv("pw")
            .contains(&"--era".to_owned()));
    }

    /// A world's status document, read as the launcher reads it, reaches the Dereth client's era
    /// view through the command line: the era, and every system the document lists over that
    /// era's table.
    #[test]
    fn the_era_and_systems_a_status_announces_reach_the_dereth_clients_era_view() {
        use dereth_primitives::{EraFeatures, EraId};
        // An Infiltration world whose configuration turns aetheria on and trade off.
        let world_has = EraFeatures {
            aetheria: true,
            trade: false,
            ..EraId::Infiltration.features()
        };
        let listed: Vec<String> = world_has
            .iter()
            .map(|(name, on)| format!("\"{name}\":{on}"))
            .collect();
        let doc = format!(
            "{{\"world_open\":true,\"era\":\"infiltration\",\"features\":{{{}}}}}",
            listed.join(",")
        );
        let live = crate::status::parse_world_document(doc.as_bytes()).unwrap();
        let (mut w, i) = (world(Emulator::Empyrean), inst(ClientKind::Dereth));
        // As the launcher's world list takes them from the live status.
        w.era.clone_from(&live.era);
        w.era_features.clone_from(&live.era_features);
        let argv = plan(&req(&w, &i)).unwrap().argv("pw");
        let cfg = dereth_client_runtime::config::Config::from_args_and_prefs_with(
            &argv,
            &dereth_client_runtime::config::Preferences::default(),
        )
        .unwrap();
        // As the client's start-up hands them to its era view.
        let view = dereth_client_contract::EraView {
            era: cfg.era.expect("an announced era"),
            era_announced: true,
            announced_features: cfg.era_features,
            ..Default::default()
        };
        assert_eq!(view.era, EraId::Infiltration);
        assert_eq!(view.features(), world_has);
        assert_ne!(view.features(), EraId::Infiltration.features());
    }

    #[test]
    fn retail_on_ace_and_empyrean_uses_the_host_port_form_and_its_own_folder() {
        for emu in [Emulator::Ace, Emulator::Empyrean, Emulator::Unknown] {
            let (w, i) = (world(emu), inst(ClientKind::Retail));
            let p = plan(&req(&w, &i)).unwrap();
            assert_eq!(
                p.argv("pw"),
                ["-a", "player", "-v", "pw", "-h", "eulmore.example:19000"],
                "{emu:?}"
            );
            assert_eq!(p.exe, PathBuf::from("/games/ac/acclient.exe"));
            assert_eq!(p.working_dir, PathBuf::from("/games/ac"));
        }
    }

    #[test]
    fn retail_on_gdle_uses_its_own_form() {
        let (w, i) = (world(Emulator::Gdle), inst(ClientKind::Retail));
        let p = plan(&req(&w, &i)).unwrap();
        assert_eq!(
            p.argv("pw"),
            ["-h", "eulmore.example", "-p", "19000", "-a", "player:pw"]
        );
    }

    #[test]
    fn the_redacted_form_never_carries_the_password() {
        let (w, i) = (world(Emulator::Gdle), inst(ClientKind::Retail));
        assert!(plan(&req(&w, &i))
            .unwrap()
            .redacted()
            .ends_with("-a player:***"));
        let (w, i) = (world(Emulator::Empyrean), inst(ClientKind::Dereth));
        let shown = plan(&req(&w, &i)).unwrap().redacted();
        assert!(
            shown.contains("-v ***") && !shown.contains("hunter2"),
            "{shown}"
        );
    }

    #[test]
    fn operator_arguments_come_first_and_a_world_with_no_address_is_refused() {
        let (w, i) = (world(Emulator::Ace), inst(ClientKind::Retail));
        let extra = ["-usemem".to_owned()];
        let mut r = req(&w, &i);
        r.extra_args = &extra;
        assert_eq!(plan(&r).unwrap().argv("p")[0], "-usemem");

        let mut w = world(Emulator::Ace);
        w.endpoint = None;
        assert_eq!(plan(&req(&w, &i)), Err(PlanError::NoEndpoint));
    }

    #[test]
    fn paths_with_spaces_are_quoted_in_the_shown_form() {
        let (w, mut i) = (world(Emulator::Ace), inst(ClientKind::Retail));
        i.path = PathBuf::from("/Turbine/Asheron's Call");
        let shown = plan(&req(&w, &i)).unwrap().redacted();
        assert!(
            shown.starts_with("\"/Turbine/Asheron's Call") && shown.contains("acclient.exe\" "),
            "{shown}"
        );
    }
}
