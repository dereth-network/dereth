//! Vectors: fixtures/vectors/
//! Every golden vector file under empyrean/fixtures/vectors matches the schema; tokens/exceptions
//! decode; violations rejected.
//! Fixture: checked-in ACE JSON vectors.

use std::collections::BTreeSet;

use empyrean_common::vectors::{self, ACE_COMMIT, GENERATOR_PREFIX};

#[test]
fn every_vector_file_matches_the_schema() {
    let files = vectors::all_files().expect("list fixtures/vectors");
    assert!(
        !files.is_empty(),
        "no vector files under {}",
        vectors::vectors_dir().display()
    );

    let mut areas = BTreeSet::new();
    let mut cases = 0usize;
    for path in &files {
        let file = vectors::load(path).unwrap_or_else(|e| panic!("{e}"));
        let area = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        assert_eq!(file.source.ace_commit, ACE_COMMIT, "{}", path.display());
        assert_eq!(file.source.culture, "en-US", "{}", path.display());
        assert!(
            !file.source.member.is_empty(),
            "{}: empty member",
            path.display()
        );
        assert!(
            !file.source.sdk.is_empty() && !file.source.runtime.is_empty(),
            "{}",
            path.display()
        );
        assert_eq!(
            file.generator,
            format!("{GENERATOR_PREFIX}{area}"),
            "{}: generator names another area",
            path.display()
        );
        assert!(!file.cases.is_empty(), "{}: no cases", path.display());
        cases += file.cases.len();
        areas.insert(area.to_owned());
    }

    for area in ["dotnet", "random", "entity", "formulas"] {
        assert!(areas.contains(area), "area {area} has no vector files");
    }
    assert!(cases > 0);
}

#[test]
fn tokens_and_exceptions_decode() {
    use serde_json::json;

    assert!(vectors::f64_of(&json!("NaN")).is_some_and(f64::is_nan));
    assert_eq!(vectors::f64_of(&json!("Infinity")), Some(f64::INFINITY));
    assert_eq!(
        vectors::f64_of(&json!("-Infinity")),
        Some(f64::NEG_INFINITY)
    );
    assert!(vectors::f64_of(&json!("nan")).is_none());
    let neg_zero = vectors::f64_of(&serde_json::from_str("-0.0").unwrap()).unwrap();
    assert!(vectors::same_f64(neg_zero, -0.0) && !vectors::same_f64(neg_zero, 0.0));
    // serde_json's default parser reads this Math.Round grid value one ulp low; the
    // float_roundtrip feature (enabled for the vector tests) reads it exactly.
    let v: serde_json::Value = serde_json::from_str("4503599627370495.5").unwrap();
    assert_eq!(vectors::f64_of(&v), Some(4_503_599_627_370_495.5));
    assert_eq!(vectors::f32_of(&json!(0.10000000149011612)), Some(0.1f32));
    assert_eq!(
        vectors::throws(&json!({"throws": "System.OverflowException"})),
        Some("System.OverflowException")
    );
    assert_eq!(vectors::throws(&json!({"throws": "x", "y": 1})), None);
    assert_eq!(vectors::throws(&json!(3)), None);
}

#[test]
fn schema_violations_are_rejected() {
    let p = std::path::Path::new("t.json");
    let good = r#"{"cases":[{"in":{"x":1},"out":2}],"generator":"ace-vectors dotnet","source":{"ace_commit":"a","culture":"en-US","file":null,"member":"m","runtime":"r","sdk":"s"}}"#;
    assert!(vectors::parse(p, good).is_ok());
    for bad in [
        good.replace(r#""out":2"#, r#""out":2,"extra":1"#),
        good.replace(r#"{"x":1}"#, "1"),
        good.replace(r#""file":null"#, r#""file":"references/x.cs""#),
        good.replace("ace-vectors dotnet", "dotnet"),
        good.replace(r#","sdk":"s""#, ""),
        good.replace(r#""member":"m""#, r#""member":"m","bogus":1"#),
    ] {
        assert!(vectors::parse(p, &bad).is_err(), "accepted: {bad}");
    }
}
