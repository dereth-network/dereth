//! Behaviour: none (tooling: a world repository's rules shown and changed at a layer, with the server's vocabulary)
//!
//! Vectors: `row_sql_pair/schema.json`, the server's schema, its `rules` member and rules row kind.

use std::path::Path;

use lyceum_foundry::content::Schema;
use lyceum_foundry::layer_rows;
use lyceum_foundry::repo::{Repo, RowSchema};
use lyceum_foundry::rules::{self, Change, RulesVocabulary};

use crate::common::Fixture;

fn vocabulary() -> (RowSchema, RulesVocabulary) {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cpu/world/row_sql_pair/schema.json");
    let schema = Schema::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    let rows = RowSchema::of_schema(&serde_json::json!({ "rows": schema.rows.clone().unwrap() }))
        .unwrap()
        .unwrap();
    let words = RulesVocabulary::of_schema(schema.rules.as_ref(), &rows)
        .unwrap()
        .expect("the server's rules vocabulary");
    (rows, words)
}

/// The server's vocabulary names every era, every system with what it is, and the choices; a
/// repository's rules are each the profile's but where a layer sets them. A change in the patch is
/// its row of that one rule, and the world folded again plays it, set by the patch; put back to the
/// era's, a rule the base sets is unset by the patch, and the era's again.
#[test]
fn a_rule_changed_in_a_patch_is_the_patchs_and_the_world_plays_it() {
    let (rows, words) = vocabulary();
    for era in ["launch", "infiltration", "eor", "newdereth"] {
        assert!(words.profiles.contains_key(era), "{era}");
    }
    assert!(words.features.len() > 50);
    assert!(words.features.iter().all(|(_, about)| !about.is_empty()));
    assert!(words.choices["loot_rules"].contains(&"infiltration".to_owned()));

    let f = Fixture::new("repo-rules");
    f.write(
        "repo/base/library/pack.toml",
        "name = \"library\"\nversion = \"1\"\n",
    );
    f.write(
        "repo/base/library/rules.toml",
        "[[rules]]\nmax_level = 50\n\n[rules.features]\nratings = true\n",
    );
    f.write(
        "repo/patches/2027-01-first/patch.toml",
        "id = \"2027-01-first\"\ntitle = \"First\"\ndate = \"2027-01-01\"\nafter = \"\"\npacks = [\"main\"]\n",
    );
    f.write(
        "repo/patches/2027-01-first/packs/main/pack.toml",
        "name = \"main\"\nversion = \"1\"\n",
    );
    let world = f.write(
        "repo/world.toml",
        "format_version = 1\ngame = \"ac1\"\nera = \"newdereth\"\npacks = [\"base/library\"]\nlineage = [\"2027-01-first\"]\n",
    );
    let repo = Repo::open(&world).unwrap();
    let profile = repo.era.name();
    // The repository read again, and its rules at the patch.
    let at = || {
        let w = Repo::open(&world).unwrap().fold(&rows, 1).unwrap();
        rules::shown(&words, profile, w.items.get(rules::KIND))
    };
    let get =
        |shown: &[rules::ShownRule], f: &str| shown.iter().find(|r| r.field == f).cloned().unwrap();

    let base = at();
    assert!(base
        .iter()
        .filter(|r| r.field.starts_with("features."))
        .all(|r| matches!(r.profile, Some(toml::Value::Boolean(_)))));
    assert_eq!(
        get(&base, "features.ratings").set,
        Some((toml::Value::Boolean(true), "base".into()))
    );
    let housing = get(&base, "features.housing");
    assert_eq!(housing.set, None, "no layer sets it: the era's");

    // The patch turns housing over.
    let below = repo
        .fold(&rows, 0)
        .unwrap()
        .items
        .get(rules::KIND)
        .map(|i| i.row(&rows));
    let flipped = !housing.value().and_then(toml::Value::as_bool).unwrap();
    let file = repo.layers[1].packs[0].dir.join("rules.toml");
    let row = rules::changed(
        &rows,
        below.as_ref(),
        None,
        "features.housing",
        &Change::To(flipped.into()),
    );
    let text = layer_rows::write_row(&rows, rules::KIND, None, rules::KIND, row.as_ref())
        .unwrap()
        .unwrap();
    std::fs::write(&file, &text).unwrap();
    assert_eq!(
        text,
        format!("[[rules]]\n\n[rules.features]\nhousing = {flipped}\n")
    );
    let patched = at();
    assert_eq!(
        get(&patched, "features.housing").set,
        Some((flipped.into(), "2027-01-first".into()))
    );
    assert_eq!(
        get(&patched, "max_level").value(),
        Some(&toml::Value::Integer(50))
    );

    // Ratings back to the era's, in the patch: unset there, and the era's again.
    let layer = layer_rows::row_in(&rows, rules::KIND, &text, rules::KIND).unwrap();
    let row = rules::changed(
        &rows,
        below.as_ref(),
        layer.as_ref(),
        "features.ratings",
        &Change::Profile,
    );
    let text = layer_rows::write_row(&rows, rules::KIND, Some(&text), rules::KIND, row.as_ref())
        .unwrap()
        .unwrap();
    std::fs::write(&file, &text).unwrap();
    let back = at();
    let ratings = get(&back, "features.ratings");
    assert_eq!(ratings.set, None, "{text}");
    assert_eq!(ratings.value(), ratings.profile.as_ref());
}
