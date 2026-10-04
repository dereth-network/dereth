//! The pre-launch check: everything a world would turn the player away for, caught before the
//! client connects, each with its fix.
//!
//! A world compares the client's logon version and its dat iterations during the first seconds of
//! a session. ACE and Empyrean then boot a client whose dats are newer and warn (or patch) one whose
//! dats are older; GDLE sends a critical error on any mismatch. The launcher reads the same numbers
//! from the files on disk, so it can say the same thing ten seconds earlier, with a fix attached.
//!
//! This is a pure function. It reads no files: the caller hands it the installation and dat set as
//! last read (see [`crate::datset::DatSet::refresh`]), which is what keeps a check on an unchanged
//! setup to a few microseconds.
//!
//! The verdicts follow one rule: **block** what the server is known to refuse, **warn**
//! about what merely degrades, and never let the launcher's own data lock the player out. The
//! registry can be wrong, so every block can be overridden by an advanced setting.

use serde::{Deserialize, Serialize};

use crate::datset::{Comparison, DatOrigin, DatRole, DatSet, Iterations, Mismatch};
use crate::install::{ClientKind, Installation};
use crate::world::{Emulator, World, WorldState, END_OF_RETAIL_NET_VERSION};

/// How bad a finding is. Ordered, so the worst of a report is its `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Warn,
    Block,
}

/// Which check a finding comes from. The interface keys the compatibility strip off these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    Client,
    Data,
    CustomData,
    DatSetOwnership,
    WorldStatus,
    SingleInstance,
    Highres,
}

/// What the player can do about a finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "url")]
pub enum Fix {
    /// Pick the other client: the Dereth client instead of the retail one, or the other way round.
    ChooseClient,
    /// Pick another dat set.
    ChooseDatSet,
    /// Copy the shared set into one this world may write to.
    CreatePrivateCopy,
    /// The community guide to getting the end-of-retail dats.
    DatGuide,
    /// The world's own guide or website.
    OpenWorldGuide(String),
    /// The world's custom data download.
    GetCustomDats(String),
    /// Hash a downloaded custom set against what the world published.
    VerifyCustomDats,
    /// Ask for the world's status again.
    Retry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub id: CheckId,
    pub verdict: Verdict,
    /// One line, the headline: "Data files don't match this world".
    pub title: String,
    /// The specifics, in the player's terms: "This world uses custom data: portal 2090 (yours: 2072)."
    pub detail: String,
    pub fixes: Vec<Fix>,
}

/// A client the launcher started and that is still running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningClient {
    pub world_slug: String,
    pub account: String,
    pub pid: u32,
    pub kind: ClientKind,
    pub multi_instance: bool,
}

/// Everything the check looks at.
#[derive(Debug, Clone, Copy)]
pub struct CheckInput<'a> {
    pub world: &'a World,
    pub install: &'a Installation,
    /// The dats the client will actually open: the chosen set for the Dereth client, and for a
    /// retail client the dats beside it.
    pub dats: Option<&'a DatSet>,
    /// What the world's live status document says its dats are. When present it wins over the
    /// registry, because it is what the server will compare.
    pub live_expected: Option<Iterations>,
    pub running: &'a [RunningClient],
}

/// The findings for one launch, worst first.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn worst(&self) -> Verdict {
        self.findings
            .iter()
            .map(|f| f.verdict)
            .max()
            .unwrap_or(Verdict::Pass)
    }

    /// Nothing to show: PLAY goes straight to the client.
    pub fn is_clear(&self) -> bool {
        self.worst() == Verdict::Pass
    }

    /// Whether PLAY may proceed from the check screen. Warnings always may; blocks only with the
    /// advanced override on.
    pub fn can_play(&self, allow_override: bool) -> bool {
        self.worst() < Verdict::Block || allow_override
    }

    pub fn get(&self, id: CheckId) -> Option<&Finding> {
        self.findings.iter().find(|f| f.id == id)
    }

    /// The worst verdict among findings of one kind, for the compatibility strip.
    pub fn verdict_of(&self, id: CheckId) -> Option<Verdict> {
        self.findings
            .iter()
            .filter(|f| f.id == id)
            .map(|f| f.verdict)
            .max()
    }
}

/// Which files a world compares. GDLE's comparison covers portal and cell and skips the local file,
/// and the launcher mirrors the server rather than being stricter than it. High resolution is
/// compared only when the player has the file, because the client reports only what it opened.
fn compared_roles(world: &World, have: &Iterations) -> Vec<DatRole> {
    let mut roles = vec![DatRole::Portal, DatRole::Cell];
    if world.emulator != Emulator::Gdle {
        roles.push(DatRole::Local);
    }
    if have.highres.is_some() {
        roles.push(DatRole::Highres);
    }
    roles
}

fn describe(ms: &[Mismatch], yours_first: bool) -> String {
    ms.iter()
        .map(|m| {
            if yours_first {
                format!("{} {} (world: {})", m.role.label(), m.have, m.want)
            } else {
                format!("{} {} (yours: {})", m.role.label(), m.want, m.have)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Run the check.
pub fn check(input: &CheckInput<'_>) -> Report {
    let mut f = Vec::new();
    let w = input.world;

    client_finding(input, &mut f);
    data_findings(input, &mut f);

    match w.state {
        WorldState::Offline => f.push(Finding {
            id: CheckId::WorldStatus,
            verdict: Verdict::Warn,
            title: format!("{} looks offline", w.name),
            detail: "Its status can be out of date, so you can still try.".into(),
            fixes: vec![Fix::Retry],
        }),
        WorldState::Starting => f.push(Finding {
            id: CheckId::WorldStatus,
            verdict: Verdict::Warn,
            title: format!("{} is starting up", w.name),
            detail: "It is not letting anyone in yet.".into(),
            fixes: vec![Fix::Retry],
        }),
        WorldState::Online | WorldState::High => {
            f.push(pass(CheckId::WorldStatus, format!("{} is online", w.name)))
        }
        WorldState::Unknown => f.push(pass(
            CheckId::WorldStatus,
            format!("{} has not reported its status; it may still be up", w.name),
        )),
    }

    if !input.install.multi_instance
        && input
            .running
            .iter()
            .any(|r| r.kind == ClientKind::Retail && !r.multi_instance)
    {
        f.push(Finding {
            id: CheckId::SingleInstance,
            verdict: Verdict::Warn,
            title: "Another copy of this client is running".into(),
            detail: "This build allows one copy at a time, so a second will not start.".into(),
            fixes: vec![Fix::ChooseClient],
        });
    }

    // Stable, so findings of equal weight keep the order above.
    f.sort_by_key(|x| core::cmp::Reverse(x.verdict));
    Report { findings: f }
}

fn pass(id: CheckId, title: String) -> Finding {
    Finding {
        id,
        verdict: Verdict::Pass,
        title,
        detail: String::new(),
        fixes: Vec::new(),
    }
}

fn client_finding(input: &CheckInput<'_>, f: &mut Vec<Finding>) {
    let (w, i) = (input.world, input.install);
    let name = i.display_name();
    if w.accepts(&i.client_id, i.net_version.as_deref()) {
        f.push(pass(CheckId::Client, format!("Client {name} is accepted")));
        return;
    }
    let guide = w.links.guide.clone().or_else(|| w.links.website.clone());
    let mut fixes = vec![Fix::ChooseClient];
    fixes.extend(guide.map(Fix::OpenWorldGuide));

    if !w.accepted_clients.is_empty() {
        f.push(Finding {
            id: CheckId::Client,
            verdict: Verdict::Block,
            title: format!("{} does not accept this client", w.name),
            detail: format!("{name} is not on the world's list of accepted clients."),
            fixes,
        });
        return;
    }
    // No explicit list: the rule is the end-of-retail logon version.
    let detail = match i.net_version.as_deref() {
        Some(v) => format!("{name} logs on as version {v}; the world expects {END_OF_RETAIL_NET_VERSION}."),
        None => format!(
            "The logon version of {name} is not known, and the world expects {END_OF_RETAIL_NET_VERSION}."
        ),
    };
    // Every emulator refuses a different version at login. A world whose software is not known
    // might not, so it gets a warning rather than a wall.
    let verdict = if w.emulator.requires_end_of_retail_protocol() {
        Verdict::Block
    } else {
        Verdict::Warn
    };
    f.push(Finding {
        id: CheckId::Client,
        verdict,
        title: "This client may not be able to log on".into(),
        detail,
        fixes,
    });
}

fn data_findings(input: &CheckInput<'_>, f: &mut Vec<Finding>) {
    let w = input.world;
    let Some(set) = input.dats else {
        f.push(Finding {
            id: CheckId::Data,
            verdict: Verdict::Block,
            title: "No data files chosen".into(),
            detail: "The client needs the game's four .dat files.".into(),
            fixes: vec![Fix::ChooseDatSet, Fix::DatGuide],
        });
        return;
    };
    let have = set.iterations();

    // A file that is there but unreadable is named: "portal is damaged" is actionable, a generic
    // mismatch is not.
    let broken: Vec<_> = set.files.iter().filter(|x| x.error.is_some()).collect();
    if !broken.is_empty() {
        f.push(Finding {
            id: CheckId::Data,
            verdict: Verdict::Block,
            title: "Some data files cannot be read".into(),
            detail: broken
                .iter()
                .map(|x| format!("{}: {}", x.file_name, x.error.as_deref().unwrap_or("")))
                .collect::<Vec<_>>()
                .join("; "),
            fixes: vec![Fix::ChooseDatSet, Fix::DatGuide],
        });
        return;
    }
    let missing: Vec<_> = [DatRole::Portal, DatRole::Cell, DatRole::Local]
        .into_iter()
        .filter(|r| have.get(*r).is_none())
        .collect();
    if !missing.is_empty() {
        f.push(Finding {
            id: CheckId::Data,
            verdict: Verdict::Block,
            title: "Data files are missing".into(),
            detail: format!(
                "Not found in {}: {}.",
                set.path.display(),
                missing
                    .iter()
                    .map(|r| r.file_name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            fixes: vec![Fix::ChooseDatSet, Fix::DatGuide],
        });
        return;
    }

    ownership_finding(w, input.install.kind, set, f);

    let roles = compared_roles(w, &have);
    if let Some(custom) = &w.dats.custom {
        let wanted = input.live_expected.unwrap_or(custom.iterations);
        let c = have.compare(&wanted, &roles);
        if c.matches() {
            f.push(pass(
                CheckId::CustomData,
                format!("{}'s own data files are in place", w.name),
            ));
        } else {
            let mut ms = c.newer.clone();
            ms.extend(c.older.iter().copied());
            f.push(Finding {
                id: CheckId::CustomData,
                verdict: Verdict::Block,
                title: "Data files don't match this world".into(),
                detail: format!(
                    "This world uses custom data: {}. Without them the server will turn you away after you connect.",
                    describe(&ms, false)
                ),
                fixes: vec![Fix::GetCustomDats(custom.url.clone()), Fix::VerifyCustomDats],
            });
        }
        highres_finding(w, &have, f);
        return;
    }

    // The world's live word, then the registry's, then the assumption every emulator ships with.
    let (expected, assumed) = match input.live_expected.or(w.dats.expected) {
        Some(e) => (e, false),
        None => (Iterations::END_OF_RETAIL, true),
    };
    let c: Comparison = have.compare(&expected, &roles);
    // A block that rests on an assumption is only a warning: the launcher does not refuse a player
    // on the strength of something nobody told it.
    let block = if assumed {
        Verdict::Warn
    } else {
        Verdict::Block
    };
    let assumed_note = if assumed {
        " This world has not published its data version, so end of retail is assumed."
    } else {
        ""
    };

    if !c.newer.is_empty() {
        f.push(Finding {
            id: CheckId::Data,
            verdict: block,
            title: "Your data files are newer than this world's".into(),
            detail: format!(
                "{}. The server would turn you away with \"Your DAT files are newer than expected\".{assumed_note}",
                describe(&c.newer, true)
            ),
            fixes: vec![Fix::ChooseDatSet],
        });
    }
    if !c.older.is_empty() {
        let private_here = set.owner_world() == Some(w.slug.as_str());
        if w.patches() && private_here {
            f.push(Finding {
                id: CheckId::Data,
                verdict: Verdict::Warn,
                title: "The world will update these files".into(),
                detail: format!(
                    "{}. It patches them when you connect.",
                    describe(&c.older, true)
                ),
                fixes: Vec::new(),
            });
        } else if !w.patches() {
            f.push(Finding {
                id: CheckId::Data,
                verdict: block,
                title: "Your data files are older than this world's".into(),
                detail: format!(
                    "{}. This world does not update them, and would turn you away with \"Your DAT files are incomplete\".{assumed_note}",
                    describe(&c.older, true)
                ),
                fixes: vec![Fix::ChooseDatSet, Fix::DatGuide],
            });
        }
        // Older on a patching world with a set it may not write to is the ownership finding's job.
    }
    if c.matches() {
        let label = if expected.is_end_of_retail() {
            " = end of retail"
        } else {
            ""
        };
        f.push(pass(
            CheckId::Data,
            format!("Data {}{label} matches", have.label()),
        ));
    }
    highres_finding(w, &have, f);
}

/// A world that writes to dats, or ships its own, must never be handed a set another world uses.
fn ownership_finding(w: &World, kind: ClientKind, set: &DatSet, f: &mut Vec<Finding>) {
    // A world that does neither may use any set; one patched by its owner shows up in the
    // iteration comparison instead.
    if !w.needs_private_dats() {
        return;
    }
    // A retail client plays with the dats beside it and nothing else, so it cannot be given a set
    // of the world's own. The Dereth client can.
    if kind == ClientKind::Retail {
        let detail = if w.dats.custom.is_some() {
            format!("{} ships its own data files, and the retail client only plays with the ones beside it. Play this world with the Dereth client.", w.name)
        } else {
            format!(
                "{} updates data files over the network, and the retail client would take the update into the files beside it. Play this world with the Dereth client and a private copy.",
                w.name
            )
        };
        f.push(Finding {
            id: CheckId::DatSetOwnership,
            verdict: Verdict::Block,
            title: "This world needs data files of its own".into(),
            detail,
            fixes: vec![Fix::ChooseClient],
        });
        return;
    }
    let ok = match &set.origin {
        DatOrigin::World { slug } => slug == &w.slug,
        DatOrigin::Custom { sha256 } => w
            .dats
            .custom
            .as_ref()
            .and_then(|c| c.sha256.as_deref())
            .is_none_or(|h| h.eq_ignore_ascii_case(sha256)),
        DatOrigin::Shared | DatOrigin::Unassigned => false,
    };
    if ok {
        return;
    }
    let (title, detail) = match &set.origin {
        DatOrigin::World { slug } => (
            "These data files belong to another world".to_owned(),
            format!("This set is {slug}'s private copy. {} needs its own.", w.name),
        ),
        _ if w.patches() => (
            "This world would change the shared data files".to_owned(),
            format!(
                "{} updates data files over the network. Give it a private copy (about 1.4 GB) so the files your other worlds use stay as they are.",
                w.name
            ),
        ),
        _ => (
            "This world needs its own data files".to_owned(),
            format!("{} ships custom data files, which go in a set of their own.", w.name),
        ),
    };
    let fixes = if w.dats.custom.is_some() {
        vec![Fix::ChooseDatSet]
    } else {
        vec![Fix::CreatePrivateCopy, Fix::ChooseDatSet]
    };
    f.push(Finding {
        id: CheckId::DatSetOwnership,
        verdict: Verdict::Block,
        title,
        detail,
        fixes,
    });
}

fn highres_finding(w: &World, have: &Iterations, f: &mut Vec<Finding>) {
    if w.dats.highres_required == Some(true) && have.highres.is_none() {
        f.push(Finding {
            id: CheckId::Highres,
            verdict: Verdict::Warn,
            title: "client_highres.dat is missing".into(),
            detail: "The client skips it without a word, so the world will look lower-detail."
                .into(),
            fixes: vec![Fix::ChooseDatSet, Fix::DatGuide],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datset::DatFileState;
    use crate::install::IdentifiedBy;
    use crate::world::{CustomDats, Endpoint};
    use std::path::PathBuf;

    fn world() -> World {
        let mut w = World::new("eulmore", "Eulmore");
        w.emulator = Emulator::Empyrean;
        w.state = WorldState::Online;
        w.endpoint = Some(Endpoint {
            address: "h".into(),
            port: 1,
            transport: None,
        });
        w.dats.expected = Some(Iterations::END_OF_RETAIL);
        w
    }

    fn install(client_id: &str, net: Option<&str>, kind: ClientKind, multi: bool) -> Installation {
        Installation {
            id: "i".into(),
            path: PathBuf::from("C:/ac"),
            exe: "acclient.exe".into(),
            kind,
            client_id: client_id.into(),
            version: Some("00.00.11.6096".into()),
            build_date: Some("2015-06-12".into()),
            net_version: net.map(Into::into),
            identified_by: IdentifiedBy::ExeSha256,
            modifications: vec![],
            multi_instance: multi,
            own_dats: Iterations::default(),
            verified_at: None,
            manifest_result: None,
        }
    }

    fn retail() -> Installation {
        install("acclient-6096", Some("1802"), ClientKind::Retail, true)
    }

    fn dereth() -> Installation {
        install("dereth", Some("1802"), ClientKind::Dereth, true)
    }

    fn set(origin: DatOrigin, it: Iterations) -> DatSet {
        DatSet {
            id: "s".into(),
            path: PathBuf::from("C:/ac"),
            kind: crate::datset::SetKind::Modern,
            origin,
            files: DatRole::ALL
                .iter()
                .filter_map(|r| {
                    it.get(*r).map(|n| DatFileState {
                        role: *r,
                        file_name: r.file_name().into(),
                        size: 1,
                        modified: 0,
                        read_only: true,
                        iterations: Some(n),
                        error: None,
                    })
                })
                .collect(),
            last_patched_by_server: None,
            created_by_launcher: false,
        }
    }

    fn run(w: &World, i: &Installation, s: Option<&DatSet>) -> Report {
        check(&CheckInput {
            world: w,
            install: i,
            dats: s,
            live_expected: None,
            running: &[],
        })
    }

    #[test]
    fn a_matching_setup_is_clear_and_launches_straight_away() {
        let s = set(DatOrigin::Shared, Iterations::END_OF_RETAIL);
        let r = run(&world(), &retail(), Some(&s));
        assert!(r.is_clear(), "{r:#?}");
        assert!(r
            .get(CheckId::Data)
            .unwrap()
            .title
            .contains("end of retail"));
    }

    #[test]
    fn newer_dats_block_because_the_server_boots_them() {
        let mut it = Iterations::END_OF_RETAIL;
        it.portal = Some(2080);
        let s = set(DatOrigin::Shared, it);
        let r = run(&world(), &retail(), Some(&s));
        assert_eq!(r.worst(), Verdict::Block);
        let d = r.get(CheckId::Data).unwrap();
        assert!(
            d.detail.contains("portal 2080 (world: 2072)"),
            "{}",
            d.detail
        );
        assert!(
            r.can_play(true),
            "the advanced override always lets the player through"
        );
        assert!(!r.can_play(false));
    }

    #[test]
    fn older_dats_block_on_a_world_that_does_not_patch() {
        let mut it = Iterations::END_OF_RETAIL;
        it.cell = Some(900);
        let r = run(&world(), &retail(), Some(&set(DatOrigin::Shared, it)));
        assert_eq!(r.verdict_of(CheckId::Data), Some(Verdict::Block));
        assert!(r.get(CheckId::Data).unwrap().fixes.contains(&Fix::DatGuide));
    }

    #[test]
    fn older_dats_on_a_patching_world_warn_only_on_its_own_private_set() {
        let mut w = world();
        w.dats.patches_over_wire = Some(true);
        let mut it = Iterations::END_OF_RETAIL;
        it.portal = Some(2000);

        let own = set(
            DatOrigin::World {
                slug: "eulmore".into(),
            },
            it,
        );
        let r = run(&w, &dereth(), Some(&own));
        assert_eq!(r.worst(), Verdict::Warn, "{r:#?}");
        assert!(r.can_play(false));

        let shared = set(DatOrigin::Shared, it);
        let r = run(&w, &dereth(), Some(&shared));
        assert_eq!(r.verdict_of(CheckId::DatSetOwnership), Some(Verdict::Block));
        assert!(r
            .get(CheckId::DatSetOwnership)
            .unwrap()
            .fixes
            .contains(&Fix::CreatePrivateCopy));
    }

    #[test]
    fn a_patching_world_never_gets_the_shared_set_even_when_it_matches() {
        let mut w = world();
        w.dats.patches_over_wire = Some(true);
        let r = run(
            &w,
            &dereth(),
            Some(&set(DatOrigin::Shared, Iterations::END_OF_RETAIL)),
        );
        assert_eq!(r.worst(), Verdict::Block);
    }

    #[test]
    fn another_worlds_private_set_is_refused_by_a_patching_world() {
        let mut w = world();
        w.dats.patches_over_wire = Some(true);
        let s = set(
            DatOrigin::World {
                slug: "coldeve".into(),
            },
            Iterations::END_OF_RETAIL,
        );
        let r = run(&w, &dereth(), Some(&s));
        assert!(r
            .get(CheckId::DatSetOwnership)
            .unwrap()
            .detail
            .contains("coldeve"));
    }

    #[test]
    fn a_custom_dat_world_says_what_it_wants_and_where_to_get_it() {
        let mut w = world();
        let mut want = Iterations::END_OF_RETAIL;
        want.portal = Some(2090);
        w.dats.custom = Some(CustomDats {
            url: "https://frostfell.example/dats".into(),
            sha256: None,
            size: None,
            iterations: want,
            license_note: None,
        });
        let s = set(
            DatOrigin::Custom { sha256: "x".into() },
            Iterations::END_OF_RETAIL,
        );
        let r = run(&w, &dereth(), Some(&s));
        let c = r.get(CheckId::CustomData).unwrap();
        assert_eq!(c.verdict, Verdict::Block);
        assert!(
            c.detail.contains("portal 2090 (yours: 2072)"),
            "{}",
            c.detail
        );
        assert!(c
            .fixes
            .contains(&Fix::GetCustomDats("https://frostfell.example/dats".into())));
    }

    #[test]
    fn a_retail_client_on_a_world_that_needs_its_own_dats_is_pointed_at_the_dereth_client() {
        let mut w = world();
        w.dats.patches_over_wire = Some(true);
        let s = set(DatOrigin::Shared, Iterations::END_OF_RETAIL);
        let r = run(&w, &retail(), Some(&s));
        let o = r.get(CheckId::DatSetOwnership).unwrap();
        assert_eq!(o.verdict, Verdict::Block);
        assert_eq!(o.fixes, [Fix::ChooseClient]);
        assert!(o.detail.contains("Dereth client"), "{}", o.detail);
    }

    #[test]
    fn gdle_ignores_the_local_file_as_its_server_does() {
        let mut w = world();
        w.emulator = Emulator::Gdle;
        let mut it = Iterations::END_OF_RETAIL;
        it.local = Some(1);
        assert!(run(&w, &retail(), Some(&set(DatOrigin::Shared, it))).is_clear());
        w.emulator = Emulator::Ace;
        assert!(!run(&w, &retail(), Some(&set(DatOrigin::Shared, it))).is_clear());
    }

    #[test]
    fn a_world_that_has_not_said_is_compared_with_end_of_retail_but_only_warns() {
        let mut w = world();
        w.dats.expected = None;
        let mut it = Iterations::END_OF_RETAIL;
        it.portal = Some(1500);
        let r = run(&w, &retail(), Some(&set(DatOrigin::Shared, it)));
        assert_eq!(r.verdict_of(CheckId::Data), Some(Verdict::Warn));
        assert!(r.get(CheckId::Data).unwrap().detail.contains("assumed"));
    }

    #[test]
    fn the_live_status_wins_over_the_registry() {
        let w = world();
        let mut live = Iterations::END_OF_RETAIL;
        live.portal = Some(2090);
        let s = set(DatOrigin::Shared, Iterations::END_OF_RETAIL);
        let r = check(&CheckInput {
            world: &w,
            install: &retail(),
            dats: Some(&s),
            live_expected: Some(live),
            running: &[],
        });
        assert_eq!(r.verdict_of(CheckId::Data), Some(Verdict::Block));
    }

    /// An Infiltration-era world that serves the end-of-retail dats (its status document says so)
    /// is played with the end-of-retail set, by the retail client as by Dereth; any other set is
    /// blocked, as on every world.
    #[test]
    fn an_infiltration_world_on_the_end_of_retail_dats_matches_them() {
        let mut w = world();
        w.era = Some("infiltration".into());
        let live = crate::status::parse_world_document(
            br#"{"world_open":true,"era":"infiltration",
            "dats":{"portal":2072,"cell":982,"local":994,"highres":497,"patching":false},
            "client_versions":["1802"]}"#,
        )
        .unwrap();
        assert_eq!(live.era, w.era);
        let eor = set(DatOrigin::Shared, Iterations::END_OF_RETAIL);
        let r = check(&CheckInput {
            world: &w,
            install: &retail(),
            dats: Some(&eor),
            live_expected: live.dats,
            running: &[],
        });
        assert!(r.is_clear(), "{r:#?}");
        let mut older = Iterations::END_OF_RETAIL;
        older.portal = Some(2050);
        let r = check(&CheckInput {
            world: &w,
            install: &retail(),
            dats: Some(&set(DatOrigin::Shared, older)),
            live_expected: live.dats,
            running: &[],
        });
        assert_eq!(r.verdict_of(CheckId::Data), Some(Verdict::Block));
    }

    #[test]
    fn an_unlisted_client_is_blocked_with_a_way_out() {
        let mut w = world();
        w.accepted_clients = vec!["dereth".into()];
        w.links.guide = Some("https://guide".into());
        let s = set(DatOrigin::Shared, Iterations::END_OF_RETAIL);
        let r = run(&w, &retail(), Some(&s));
        let c = r.get(CheckId::Client).unwrap();
        assert_eq!(c.verdict, Verdict::Block);
        assert_eq!(
            c.fixes,
            [
                Fix::ChooseClient,
                Fix::OpenWorldGuide("https://guide".into())
            ]
        );
    }

    #[test]
    fn a_legacy_client_is_blocked_by_an_emulator_and_warned_elsewhere() {
        let old = install("acclient-4079", None, ClientKind::Retail, false);
        let s = set(DatOrigin::Shared, Iterations::END_OF_RETAIL);
        assert_eq!(
            run(&world(), &old, Some(&s)).verdict_of(CheckId::Client),
            Some(Verdict::Block)
        );
        let mut w = world();
        w.emulator = Emulator::Unknown;
        assert_eq!(
            run(&w, &old, Some(&s)).verdict_of(CheckId::Client),
            Some(Verdict::Warn)
        );
    }

    #[test]
    fn offline_warns_and_never_blocks() {
        let mut w = world();
        w.state = WorldState::Offline;
        let r = run(
            &w,
            &retail(),
            Some(&set(DatOrigin::Shared, Iterations::END_OF_RETAIL)),
        );
        assert_eq!(r.worst(), Verdict::Warn);
        assert_eq!(
            r.findings[0].id,
            CheckId::WorldStatus,
            "the worst finding comes first"
        );
    }

    #[test]
    fn a_second_stock_retail_client_is_warned_about() {
        let stock = install("acclient-4186", Some("1802"), ClientKind::Retail, false);
        let running = [RunningClient {
            world_slug: "x".into(),
            account: "a".into(),
            pid: 1,
            kind: ClientKind::Retail,
            multi_instance: false,
        }];
        let s = set(DatOrigin::Shared, Iterations::END_OF_RETAIL);
        let r = check(&CheckInput {
            world: &world(),
            install: &stock,
            dats: Some(&s),
            live_expected: None,
            running: &running,
        });
        assert_eq!(r.verdict_of(CheckId::SingleInstance), Some(Verdict::Warn));
        // The Dereth client runs any number of copies.
        let dereth = install("dereth", Some("1802"), ClientKind::Dereth, true);
        let r = check(&CheckInput {
            world: &world(),
            install: &dereth,
            dats: Some(&s),
            live_expected: None,
            running: &running,
        });
        assert_eq!(r.verdict_of(CheckId::SingleInstance), None);
    }

    #[test]
    fn a_missing_highres_file_warns_when_the_world_wants_it() {
        let mut w = world();
        w.dats.highres_required = Some(true);
        let mut it = Iterations::END_OF_RETAIL;
        it.highres = None;
        let r = run(&w, &retail(), Some(&set(DatOrigin::Shared, it)));
        assert_eq!(r.verdict_of(CheckId::Highres), Some(Verdict::Warn));
        assert_eq!(
            r.verdict_of(CheckId::Data),
            Some(Verdict::Pass),
            "highres is not compared when absent"
        );
    }

    #[test]
    fn no_set_or_a_broken_file_blocks_with_the_file_named() {
        assert_eq!(run(&world(), &retail(), None).worst(), Verdict::Block);
        let mut s = set(DatOrigin::Shared, Iterations::END_OF_RETAIL);
        s.files[0].iterations = None;
        s.files[0].error = Some("damaged data file: the file ends early".into());
        let r = run(&world(), &retail(), Some(&s));
        assert!(r
            .get(CheckId::Data)
            .unwrap()
            .detail
            .contains("client_portal.dat"));
    }
}
