//! Divergence: V385
//! ACE: Program.CheckForServerUpdate (ACE only reports a newer ACE release; Empyrean installs its
//! own within the operator's policy)
//! Behaviour: none (a server tooling claim: which release the updater installs, not game behaviour).
//! Release versions order as semantic versions; `server.update` = off installs nothing, patch takes
//! only patch releases that change nothing but the binaries, minor also takes minor releases with
//! their migrations and pack rebuilds; majors and pre-releases are reported, never taken; a release
//! is taken only from a version it declares it upgrades from; a release that failed here, has no
//! declaration, needs a new configuration key, or needs a pack the configuration cannot rebuild is
//! reported instead.
//! Fixture: synthetic release declarations.

use empyrean_server::update::policy::{self, Candidate, Policy, Selection, Situation};
use empyrean_server::update::release::{
    ConfigChange, Databases, Declaration, Facts, Kind, WorldPack,
};
use empyrean_server::update::version::Version;

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

fn mine() -> Facts {
    Facts {
        version: "1.2.0".to_owned(),
        target: "x86_64-unknown-linux-gnu".to_owned(),
        databases: Databases { shard: 3, auth: 1 },
        world_pack: WorldPack {
            format: 1,
            schema: 100,
        },
    }
}

/// A release of `kind` that changes nothing, upgradable from 1.0.0.
fn declared(kind: Kind) -> Declaration {
    Declaration {
        kind,
        previous: None,
        upgrades_from: "1.0.0".to_owned(),
        databases: Databases { shard: 3, auth: 1 },
        world_pack: WorldPack {
            format: 1,
            schema: 100,
        },
        migrations: Vec::new(),
        world_pack_rebuild: false,
        config: Vec::new(),
        notes: String::new(),
    }
}

fn release(version: &str, d: Option<Declaration>) -> Candidate {
    Candidate {
        version: v(version),
        prerelease: v(version).is_prerelease(),
        declaration: d,
    }
}

fn select(policy: Policy, candidates: &[Candidate]) -> Selection {
    select_with(policy, candidates, &[], true)
}

fn select_with(
    policy: Policy,
    candidates: &[Candidate],
    failed: &[Version],
    can_rebuild: bool,
) -> Selection {
    policy::select(
        &Situation {
            mine: &mine(),
            policy,
            failed,
            can_rebuild_world_pack: can_rebuild,
        },
        candidates,
    )
}

fn taken(s: &Selection) -> Option<String> {
    s.take.as_ref().map(|(v, _)| v.to_string())
}

fn why(s: &Selection, version: &str) -> String {
    s.reported
        .iter()
        .find(|(r, _)| *r == v(version))
        .map(|(_, w)| w.clone())
        .unwrap_or_else(|| panic!("{version} is not reported: {s:?}"))
}

#[test]
fn versions_order_as_semantic_versions_with_pre_releases_first() {
    let mut list = [
        "1.10.0",
        "1.2.0",
        "1.2.0-rc.10",
        "1.2.0-rc.2",
        "1.2.0-beta",
        "1.2.1",
        "0.9.9",
    ]
    .map(v)
    .to_vec();
    list.sort();
    let ordered: Vec<String> = list.iter().map(ToString::to_string).collect();
    assert_eq!(
        ordered,
        [
            "0.9.9",
            "1.2.0-beta",
            "1.2.0-rc.2",
            "1.2.0-rc.10",
            "1.2.0",
            "1.2.1",
            "1.10.0"
        ]
    );
    assert_eq!(
        Version::from_tag("empyrean-v0.2.0-rc.1"),
        Some(v("0.2.0-rc.1"))
    );
    assert_eq!(Version::from_tag("dereth-v0.2.0"), None);
    for bad in ["1.2", "1.2.3.4", "01.2.3", "1.2.3-", "1.2.3-a..b", "x"] {
        assert!(Version::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn off_installs_nothing_and_reports_every_newer_release() {
    let s = select(
        Policy::Off,
        &[release("1.2.1", Some(declared(Kind::Patch)))],
    );
    assert_eq!(taken(&s), None);
    assert!(why(&s, "1.2.1").contains("\"off\""));
}

#[test]
fn patch_takes_the_newest_patch_release_that_changes_nothing() {
    let s = select(
        Policy::Patch,
        &[
            release("1.2.1", Some(declared(Kind::Patch))),
            release("1.2.2", Some(declared(Kind::Patch))),
            release("1.1.9", Some(declared(Kind::Patch))),
        ],
    );
    assert_eq!(taken(&s), Some("1.2.2".to_owned()));
    assert!(s.take.as_ref().unwrap().1.is_empty());
    assert!(why(&s, "1.2.1").contains("1.2.2 is newer"));
    assert!(
        !s.reported.iter().any(|(r, _)| *r == v("1.1.9")),
        "older releases are not newer releases"
    );
}

#[test]
fn patch_does_not_take_a_minor_release_or_a_patch_that_changes_something() {
    let mut migrating = declared(Kind::Patch);
    migrating.databases.shard = 4;
    let s = select(
        Policy::Patch,
        &[
            release("1.2.1", Some(migrating)),
            release("1.3.0", Some(declared(Kind::Minor))),
        ],
    );
    assert_eq!(taken(&s), None);
    assert!(why(&s, "1.2.1").contains("shard database is migrated from schema 3 to 4"));
    assert!(why(&s, "1.3.0").contains("takes only 1.2.x"));
}

#[test]
fn minor_takes_a_minor_release_with_its_migrations_and_pack_rebuild() {
    let mut d = declared(Kind::Minor);
    d.databases.shard = 5;
    d.world_pack.format = 2;
    d.config.push(ConfigChange {
        key: "server.old_key".to_owned(),
        change: "removed".to_owned(),
        to: None,
        note: "no longer read".to_owned(),
    });
    let s = select(Policy::Minor, &[release("1.3.0", Some(d))]);
    let (version, needs) = s.take.clone().expect("taken");
    assert_eq!(version, v("1.3.0"));
    assert_eq!(needs.migrations.len(), 1);
    assert_eq!((needs.migrations[0].from, needs.migrations[0].to), (3, 5));
    assert!(needs.world_pack_rebuild);
    assert_eq!(needs.config.len(), 1);
}

#[test]
fn majors_and_pre_releases_are_reported_and_never_taken() {
    let s = select(
        Policy::Minor,
        &[
            release("2.0.0", Some(declared(Kind::Major))),
            release("1.3.0-rc.1", Some(declared(Kind::Minor))),
        ],
    );
    assert_eq!(taken(&s), None);
    assert!(why(&s, "2.0.0").contains("major release"));
    assert!(why(&s, "1.3.0-rc.1").contains("pre-release"));

    // A pre-release published as one, whatever its version.
    let mut published = release("1.3.0", Some(declared(Kind::Minor)));
    published.prerelease = true;
    assert_eq!(taken(&select(Policy::Minor, &[published])), None);
}

#[test]
fn a_release_is_taken_only_from_the_version_it_upgrades_from() {
    let mut far = declared(Kind::Minor);
    far.upgrades_from = "1.3.0".to_owned();
    let s = select(
        Policy::Minor,
        &[
            release("1.3.0", Some(declared(Kind::Minor))),
            release("1.4.0", Some(far)),
        ],
    );
    assert_eq!(taken(&s), Some("1.3.0".to_owned()), "the stepping stone");
    assert!(why(&s, "1.4.0").contains("needs 1.3.0 or newer installed first"));
}

#[test]
fn releases_the_server_cannot_take_safely_are_reported() {
    // Failed its health check here before.
    let s = select_with(
        Policy::Minor,
        &[release("1.2.1", Some(declared(Kind::Patch)))],
        &[v("1.2.1")],
        true,
    );
    assert_eq!(taken(&s), None);
    assert!(why(&s, "1.2.1").contains("failed its health check"));

    // No declaration.
    let s = select(Policy::Minor, &[release("1.2.1", None)]);
    assert!(why(&s, "1.2.1").contains("declares nothing"));

    // A release between this one and the target without a declaration blocks the target too.
    let s = select(
        Policy::Minor,
        &[
            release("1.2.1", None),
            release("1.2.2", Some(declared(Kind::Patch))),
        ],
    );
    assert_eq!(taken(&s), None);
    assert!(why(&s, "1.2.2").contains("1.2.1"));

    // A newly required configuration key.
    let mut required = declared(Kind::Minor);
    required.config.push(ConfigChange {
        key: "server.new_key".to_owned(),
        change: "required".to_owned(),
        to: None,
        note: String::new(),
    });
    let s = select(Policy::Minor, &[release("1.3.0", Some(required))]);
    assert!(why(&s, "1.3.0").contains("server.new_key"));

    // A new pack, with no dump to rebuild it from.
    let mut pack = declared(Kind::Minor);
    pack.world_pack.schema = 101;
    let s = select_with(Policy::Minor, &[release("1.3.0", Some(pack))], &[], false);
    assert!(why(&s, "1.3.0").contains("world_base_sql"));

    // Older databases than this build's.
    let mut older = declared(Kind::Minor);
    older.databases.shard = 2;
    let s = select(Policy::Minor, &[release("1.3.0", Some(older))]);
    assert!(why(&s, "1.3.0").contains("older"));
}

#[test]
fn the_policy_names_are_the_three_the_configuration_takes() {
    assert_eq!(Policy::parse(" Patch "), Some(Policy::Patch));
    assert_eq!(Policy::parse("minor"), Some(Policy::Minor));
    assert_eq!(Policy::parse("off"), Some(Policy::Off));
    assert_eq!(Policy::parse("major"), None);
    let config = empyrean_common::master_configuration::MasterConfiguration::default();
    assert_eq!(
        empyrean_server::update::configured_policy(&config),
        Ok(Policy::Off),
        "off by default"
    );
}
