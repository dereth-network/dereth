//! ACE: Source/ACE.Database/AuthenticationDatabase.cs::AuthenticationDatabase
//! AuthenticationDatabase/AccountExtensions on both backends: create/find accounts, bcrypt work-
//! factor migration and legacy SHA512, clamp, last login and bans, base64.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::{Clock, VirtualClock};
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::vectors;
use empyrean_entity::enums::AccessLevel;
use empyrean_store::bcrypt_provider::BCryptProvider;
use empyrean_store::models::auth::account_extensions::{
    base64_decode, base64_encode, get_password_hash_legacy,
};
use empyrean_store::models::auth::Account;
use empyrean_store::{AuthDatabase, MemAuth, SqliteAuth};
use serde_json::Value;

fn config(work_factor: i32) -> AccountDefaults {
    AccountDefaults {
        password_hash_work_factor: work_factor,
        ..AccountDefaults::default()
    }
}

type Backends = Vec<(&'static str, Box<dyn AuthDatabase>)>;

fn backends(work_factor: i32) -> (Arc<VirtualClock>, Backends) {
    let clock = Arc::new(VirtualClock::new(DotNetDateTime::new_hms(
        2026, 3, 4, 5, 6, 7,
    )));
    let v: Backends = vec![
        (
            "mem",
            Box::new(MemAuth::new(config(work_factor), clock.clone())),
        ),
        (
            "sqlite",
            Box::new(SqliteAuth::open_in_memory(config(work_factor), clock.clone()).unwrap()),
        ),
    ];
    (clock, v)
}

const LOCAL: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

#[test]
fn create_and_find_accounts() {
    let (_clock, dbs) = backends(4);
    for (name, mut db) in dbs {
        let a = db
            .create_account("alpha", "pw1", AccessLevel::Player, LOCAL)
            .unwrap();
        assert_eq!(a.account_id, 1, "{name}");
        assert!(
            a.password_hash.starts_with("$2y$04$") && a.password_salt == "use bcrypt",
            "{name}"
        );
        assert_eq!(
            a.create_time,
            DotNetDateTime::new_hms(2026, 3, 4, 5, 6, 7),
            "{name}"
        );
        assert_eq!(a.create_ip, Some(vec![127, 0, 0, 1]), "{name}");
        let b = db
            .create_account(
                "beta",
                "pw2",
                AccessLevel::Admin,
                IpAddr::V6(Ipv6Addr::LOCALHOST),
            )
            .unwrap();
        assert_eq!(b.create_ip.as_ref().map(Vec::len), Some(16), "{name}");

        // The name is unique and compared case-insensitively (MySQL's collation).
        assert!(
            db.create_account("ALPHA", "x", AccessLevel::Player, LOCAL)
                .is_err(),
            "{name}"
        );
        assert_eq!(
            db.get_account_by_name("Alpha").map(|a| a.account_id),
            Some(1),
            "{name}"
        );
        assert_eq!(db.get_account_id_by_name("BETA"), 2, "{name}");
        assert_eq!(db.get_account_id_by_name("nobody"), 0, "{name}");
        assert_eq!(
            db.get_account_by_id(2).map(|a| a.account_name),
            Some("beta".into()),
            "{name}"
        );
        assert!(db.get_account_by_id(9).is_none(), "{name}");
        assert_eq!(db.get_account_count(), 2, "{name}");

        assert_eq!(
            db.get_listof_accounts_by_access_level(AccessLevel::Admin),
            vec!["beta".to_string()],
            "{name}"
        );
        assert!(
            db.update_account_access_level(1, AccessLevel::Admin),
            "{name}"
        );
        assert_eq!(
            db.get_listof_accounts_by_access_level(AccessLevel::Admin),
            vec!["alpha".to_string(), "beta".to_string()],
            "{name}"
        );

        // ACE-BUG: `.First(...)` throws for an unknown id instead of returning false.
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            db.update_account_access_level(99, AccessLevel::Admin)
        }));
        assert!(r.is_err(), "{name}");
    }
}

#[test]
fn passwords_bcrypt_work_factor_migration_and_legacy_sha512() {
    let (_clock, dbs) = backends(5);
    for (name, mut db) in dbs {
        let mut a = db
            .create_account("gamma", "secret", AccessLevel::Player, LOCAL)
            .unwrap();
        assert!(a.password_matches("secret", db.as_mut()), "{name}");
        assert!(!a.password_matches("Secret", db.as_mut()), "{name}");

        // A hash at another work factor is rehashed (ForceWorkFactorMigration) on a good password only.
        a.password_hash = BCryptProvider::hash_password("secret", 4);
        db.update_account(&a);
        assert!(!a.password_matches("wrong", db.as_mut()), "{name}");
        assert_eq!(
            BCryptProvider::get_password_work_factor(
                &db.get_account_by_id(a.account_id).unwrap().password_hash
            ),
            4,
            "{name}"
        );
        assert!(a.password_matches("secret", db.as_mut()), "{name}");
        assert_eq!(
            BCryptProvider::get_password_work_factor(
                &db.get_account_by_id(a.account_id).unwrap().password_hash
            ),
            5,
            "{name}: saved"
        );

        // A legacy SHA512 account migrates to bcrypt on its first good login.
        let mut legacy = db.get_account_by_id(a.account_id).unwrap();
        legacy.password_salt = "c2FsdA==".into();
        legacy.password_hash = get_password_hash_legacy(&legacy, "old");
        db.update_account(&legacy);
        assert!(!legacy.password_matches("new", db.as_mut()), "{name}");
        assert!(legacy.password_matches("old", db.as_mut()), "{name}");
        let stored = db.get_account_by_id(a.account_id).unwrap();
        assert_eq!(stored.password_salt, "use bcrypt", "{name}");
        assert!(stored.password_hash.starts_with("$2y$05$"), "{name}");
    }
}

#[test]
fn work_factor_is_clamped() {
    let mut a = Account::default();
    a.set_password("p", &config(2));
    assert_eq!(
        BCryptProvider::get_password_work_factor(&a.password_hash),
        4
    );
}

#[test]
fn last_login_and_bans() {
    let (clock, dbs) = backends(4);
    for (name, mut db) in dbs {
        let mut a = db
            .create_account("delta", "p", AccessLevel::Player, LOCAL)
            .unwrap();
        let admin = db
            .create_account("admin", "p", AccessLevel::Admin, LOCAL)
            .unwrap();
        a.update_last_login(IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3)), db.as_mut());
        a.update_last_login(IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3)), db.as_mut());
        let s = db.get_account_by_id(a.account_id).unwrap();
        assert_eq!(
            (s.total_times_logged_in, s.last_login_ip.clone()),
            (2, Some(vec![10, 1, 2, 3])),
            "{name}"
        );
        assert_eq!(s.last_login_time, Some(clock.utc_now()), "{name}");

        a.banned_time = Some(DotNetDateTime::new(2026, 3, 1));
        a.banned_by_account_id = Some(admin.account_id);
        a.ban_expire_time = Some(DotNetDateTime::new_hms(2026, 3, 9, 14, 5, 0));
        a.ban_reason = Some("spam".into());
        db.update_account(&a);
        let mut console = db
            .create_account("echo", "p", AccessLevel::Player, LOCAL)
            .unwrap();
        console.banned_by_account_id = Some(0);
        console.ban_expire_time = Some(DotNetDateTime::new_hms(2026, 4, 1, 0, 30, 0));
        db.update_account(&console);

        assert_eq!(
            db.get_listof_banned_accounts(),
            vec![
                "delta -- banned by account admin until server time Mar 09 2026  2:05PM -- Reason: spam".to_string(),
                "echo -- banned by CONSOLE until server time Apr 01 2026  12:30AM".to_string(),
            ],
            "{name}"
        );

        clock.advance(Duration::from_secs(6 * 86400));
        assert_eq!(
            db.get_listof_banned_accounts().len(),
            1,
            "{name}: delta's ban has expired"
        );

        console.un_ban(db.as_mut());
        let s = db.get_account_by_id(console.account_id).unwrap();
        assert!(
            s.ban_expire_time.is_none()
                && s.banned_by_account_id.is_none()
                && s.ban_reason.is_none(),
            "{name}"
        );
        assert!(db.get_listof_banned_accounts().is_empty(), "{name}");

        // The backends share the clock: wind it back for the next one.
        clock.set_utc(DotNetDateTime::new_hms(2026, 3, 4, 5, 6, 7));
    }
}

#[test]
fn legacy_sha512_matches_the_harness() {
    let file = vectors::load_named("store", "account_legacy_sha512");
    for case in &file.cases {
        let password = case.input["password"].as_str().unwrap().to_owned();
        let salt = case.input["salt"].as_str().unwrap().to_owned();
        let account = Account {
            password_salt: salt.clone(),
            ..Default::default()
        };
        match case.output.get("throws").and_then(Value::as_str) {
            Some(t) => {
                assert_eq!(t, "System.FormatException");
                let r = std::panic::catch_unwind(|| get_password_hash_legacy(&account, &password));
                assert!(r.is_err(), "{salt:?}");
            }
            None => assert_eq!(
                get_password_hash_legacy(&account, &password),
                case.output.as_str().unwrap(),
                "{password:?} {salt:?}"
            ),
        }
    }
}

#[test]
fn base64_round_trip() {
    for data in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
        assert_eq!(base64_decode(&base64_encode(data)).unwrap(), data);
    }
    assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    assert_eq!(base64_decode(" Zm9v\r\nYg== ").unwrap(), b"foob");
    assert!(base64_decode("Zm9=v").is_none());
    assert!(base64_decode("Zg=a").is_none());
}
