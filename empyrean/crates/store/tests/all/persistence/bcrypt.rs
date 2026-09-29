//! Vectors: published OpenBSD and OpenWall bcrypt vectors; empyrean/fixtures/vectors/store/bcrypt_*
//! Bcrypt equals OpenBSD and OpenWall published vectors and the ACE harness; provider hashes as
//! 2y; salt format.
//! Fixture: published OpenBSD and OpenWall vectors and checked-in ACE cases.

use empyrean_store::bcrypt::{self, hash_password};

/// (password, salt settings, expected hash): jBCrypt / OpenBSD.
const OPENBSD: &[(&str, &str, &str)] = &[
    (
        "",
        "$2a$06$DCq7YPn5Rq63x1Lad4cll.",
        "$2a$06$DCq7YPn5Rq63x1Lad4cll.TV4S6ytwfsfvkgY8jIucDrjc8deX1s.",
    ),
    (
        "",
        "$2a$08$HqWuK6/Ng6sg9gQzbLrgb.",
        "$2a$08$HqWuK6/Ng6sg9gQzbLrgb.Tl.ZHfXLhvt/SgVyWhQqgqcZ7ZuUtye",
    ),
    (
        "",
        "$2a$10$k1wbIrmNyFAPwPVPSVa/ze",
        "$2a$10$k1wbIrmNyFAPwPVPSVa/zecw2BCEnBwVS2GbrmgzxFUOqW9dk4TCW",
    ),
    (
        "a",
        "$2a$06$m0CrhHm10qJ3lXRY.5zDGO",
        "$2a$06$m0CrhHm10qJ3lXRY.5zDGO3rS2KdeeWLuGmsfGlMfOxih58VYVfxe",
    ),
    (
        "a",
        "$2a$08$cfcvVd2aQ8CMvoMpP2EBfe",
        "$2a$08$cfcvVd2aQ8CMvoMpP2EBfeodLEkkFJ9umNEfPD18.hUF62qqlC/V.",
    ),
    (
        "abc",
        "$2a$06$If6bvum7DFjUnE9p2uDeDu",
        "$2a$06$If6bvum7DFjUnE9p2uDeDu0YHzrHM6tf.iqN8.yx.jNN1ILEf7h0i",
    ),
    (
        "abc",
        "$2a$08$Ro0CUfOqk6cXEKf3dyaM7O",
        "$2a$08$Ro0CUfOqk6cXEKf3dyaM7OhSCvnwM9s4wIX9JeLapehKK5YdLxKcm",
    ),
    (
        "abcdefghijklmnopqrstuvwxyz",
        "$2a$06$.rCVZVOThsIa97pEDOxvGu",
        "$2a$06$.rCVZVOThsIa97pEDOxvGuRRgzG64bvtJ0938xuqzv18d3ZpQhstC",
    ),
    (
        "abcdefghijklmnopqrstuvwxyz",
        "$2a$08$aTsUwsyowQuzRrDqFflhge",
        "$2a$08$aTsUwsyowQuzRrDqFflhgekJ8d9/7Z3GV3UcgvzQW3J5zMyrTvlz.",
    ),
    (
        "~!@#$%^&*()      ~!@#$%^&*()PNBFRD",
        "$2a$06$fPIsBO8qRqkjj273rfaOI.",
        "$2a$06$fPIsBO8qRqkjj273rfaOI.HtSV9jLDpTbZn782DC6/t7qT67P6FfO",
    ),
    (
        "~!@#$%^&*()      ~!@#$%^&*()PNBFRD",
        "$2a$08$Eq2r4G/76Wv39MzSX262hu",
        "$2a$08$Eq2r4G/76Wv39MzSX262huzPz612MZiYHVUJe/OcOql2jo4.9UxTW",
    ),
];

/// OpenWall crypt_blowfish `wrapper.c` self-test (the `$2a$`/`$2b$`/`$2y$` entries; `$2x$` is
/// deliberately not reproduced by BCrypt.Net-Next).
const OPENWALL: &[(&str, &str)] = &[
    ("$2a$05$CCCCCCCCCCCCCCCCCCCCC.E5YPO9kmyuRGyh0XouQYb4YMJKvyOeW", "U*U"),
    ("$2a$05$CCCCCCCCCCCCCCCCCCCCC.VGOzA784oUp/Z0DY336zx7pLYAy0lwK", "U*U*"),
    ("$2a$05$XXXXXXXXXXXXXXXXXXXXXOAcXxm9kjPGEMsLznoKqmqw7tc8WCx4a", "U*U*U"),
    ("$2a$05$abcdefghijklmnopqrstuu5s2v8.iXieOjg/.AySBTTZIIVFJeBui", "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789chars after 72 are ignored"),
    ("$2a$05$CCCCCCCCCCCCCCCCCCCCC.7uG0VCzI2bS7j6ymqJi9CdcdxiRTWNy", ""),
];

#[test]
fn openbsd_vectors() {
    let mut failures = Vec::new();
    for (password, salt, expected) in OPENBSD {
        let got = hash_password(password, salt).expect("valid salt");
        if got != *expected {
            failures.push(format!("{password:?} {salt}: got {got}, want {expected}"));
        }
        assert!(bcrypt::verify(password, expected).unwrap() || got != *expected);
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn openwall_vectors() {
    let mut failures = Vec::new();
    for (hash, password) in OPENWALL {
        let got = hash_password(password, hash).expect("valid salt");
        if got != *hash {
            failures.push(format!("{password:?}: got {got}, want {hash}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

// ---------------------------------------------------------------------------------------------
// BCrypt.Net-Next 4.2.0 and ACE's BCryptProvider, from the vector harness (store/*.json)
// ---------------------------------------------------------------------------------------------

use empyrean_common::vectors;
use empyrean_store::bcrypt_provider::BCryptProvider;
use serde_json::Value;

fn throws(v: &Value) -> Option<&str> {
    v.get("throws").and_then(Value::as_str)
}

#[test]
fn harness_hash_password() {
    let file = vectors::load_named("store", "bcrypt_hash_password");
    assert!(file.cases.len() > 100);
    for case in &file.cases {
        let password = case.input["password"].as_str().unwrap();
        let salt = case.input["salt"].as_str().unwrap();
        let got = hash_password(password, salt);
        match throws(&case.output) {
            Some(t) => {
                assert_eq!(
                    got.map_err(|e| e.dotnet_type()),
                    Err(t),
                    "{password:?} {salt:?}"
                )
            }
            None => assert_eq!(
                got.as_deref(),
                Ok(case.output.as_str().unwrap()),
                "{password:?} {salt:?}"
            ),
        }
    }
}

#[test]
fn harness_verify() {
    let file = vectors::load_named("store", "bcrypt_provider_verify");
    for case in &file.cases {
        let text = case.input["text"].as_str().unwrap();
        let hash = case.input["hash"].as_str().unwrap();
        let got = bcrypt::verify(text, hash);
        match throws(&case.output) {
            Some(t) => {
                assert_eq!(
                    got.clone().map_err(|e| e.dotnet_type()),
                    Err(t),
                    "{text:?} {hash:?}"
                );
                // BCryptProvider.Verify lets the exception escape.
                let r = std::panic::catch_unwind(|| BCryptProvider::verify(text, hash));
                assert!(r.is_err());
            }
            None => {
                let want = case.output.as_bool().unwrap();
                assert_eq!(got, Ok(want), "{text:?} {hash:?}");
                assert_eq!(BCryptProvider::verify(text, hash), want);
            }
        }
    }
}

#[test]
fn harness_get_password_work_factor() {
    let file = vectors::load_named("store", "bcrypt_provider_get_password_work_factor");
    for case in &file.cases {
        let hash = case.input["hash"].as_str().unwrap().to_owned();
        match throws(&case.output) {
            Some(t) => {
                assert_eq!(
                    bcrypt::interrogate_hash(&hash).map_err(|e| e.dotnet_type()),
                    Err(t),
                    "{hash:?}"
                );
                let h = hash.clone();
                assert!(std::panic::catch_unwind(
                    move || BCryptProvider::get_password_work_factor(&h)
                )
                .is_err());
            }
            None => {
                let want = i32::try_from(case.output.as_i64().unwrap()).unwrap();
                assert_eq!(
                    BCryptProvider::get_password_work_factor(&hash),
                    want,
                    "{hash:?}"
                );
            }
        }
    }
}

#[test]
fn provider_hash_password_round_trips_as_2y() {
    let a = BCryptProvider::hash_password("Pa55w0rd!", 4);
    let b = BCryptProvider::hash_password("Pa55w0rd!", 4);
    assert!(a.starts_with("$2y$04$") && a.len() == 60, "{a}");
    assert_ne!(a, b, "fresh random salt each time");
    assert!(BCryptProvider::verify("Pa55w0rd!", &a));
    assert!(!BCryptProvider::verify("pa55w0rd!", &a));
    assert_eq!(BCryptProvider::get_password_work_factor(&a), 4);
}

#[test]
fn generate_salt_format_and_range() {
    let s = bcrypt::generate_salt_from(10, 'y', &[0u8; 16]).unwrap();
    assert_eq!(s, "$2y$10$......................");
    assert!(bcrypt::generate_salt(3, 'y').is_err());
    assert!(bcrypt::generate_salt(32, 'y').is_err());
    assert_eq!(bcrypt::generate_salt(31, 'a').unwrap().len(), 29);
}
