//! A world repository's rules (its `[[rules]]` row) as the Rules tab shows and changes them: every
//! rule the row can say, with the value of the era the world plays (its profile), the value the
//! world has at a layer, and the layer that set it.
//!
//! The vocabulary is the server's: the `rules` member of what `empyrean-import schema` writes,
//! each era's own rules written as the row writes them, the systems by name with what each is, and
//! the formula settings' names. A rule no layer sets is the profile's.
//!
//! A change is written as the layer's row of only what it changes ([`crate::layer_rows::delta`]):
//! a rule set to what the world below has is no longer the layer's, and a rule put back to the
//! profile's is unset (`remove_fields`) when a layer below sets it.

use std::collections::BTreeMap;

use toml::Value as T;

use crate::layer_rows;
use crate::repo::{RowSchema, ShownItem};

/// The rules row's kind, and its one item.
pub const KIND: &str = "rules";

/// The rules row's file in a pack.
pub const FILE: &str = "rules.toml";

/// The rules row's fields that are no map's, in the order the tab shows them.
pub const FIELDS: [&str; 11] = [
    "max_level",
    "start",
    "account_has_tod",
    "loot",
    "loot_rules",
    "starter_gear",
    "placements_on_own_land",
    "welcome_letter",
    "creation_skills",
    "training_skills",
    "heritages",
];

/// What the server says a rules row can say.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RulesVocabulary {
    /// Each era's own rules, by its name (`eor`, `newdereth`), as the row's fields.
    pub profiles: BTreeMap<String, BTreeMap<String, T>>,
    /// Each system, by name, with what it is.
    pub features: Vec<(String, String)>,
    pub formulas: Vec<String>,
    /// The names a field chooses among, by the field (`start`, `formulas.vitae`).
    pub choices: BTreeMap<String, Vec<String>>,
}

fn toml_of(v: &serde_json::Value) -> Option<T> {
    Some(match v {
        serde_json::Value::Null => return None,
        serde_json::Value::Bool(b) => T::Boolean(*b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => T::Integer(i),
            None => T::Float(n.as_f64()?),
        },
        serde_json::Value::String(s) => T::String(s.clone()),
        serde_json::Value::Array(a) => T::Array(a.iter().filter_map(toml_of).collect()),
        serde_json::Value::Object(o) => T::Table(
            o.iter()
                .filter_map(|(k, v)| Some((k.clone(), toml_of(v)?)))
                .collect(),
        ),
    })
}

impl RulesVocabulary {
    /// The vocabulary in a schema document's `rules` member (`rules`), read with the rows'
    /// vocabulary `rows`; `None` from a server too old to say.
    ///
    /// # Errors
    /// A member that doesn't read.
    pub fn of_schema(
        rules: Option<&serde_json::Value>,
        rows: &RowSchema,
    ) -> Result<Option<Self>, String> {
        let Some(rules) = rules else {
            return Ok(None);
        };
        let mut profiles = BTreeMap::new();
        for (era, row) in rules
            .get("profiles")
            .and_then(serde_json::Value::as_object)
            .ok_or("the rules member has no profiles")?
        {
            let Some(T::Table(row)) = toml_of(row) else {
                return Err(format!("profile {era} is not a row"));
            };
            profiles.insert(era.clone(), layer_rows::fields(rows, KIND, &row));
        }
        let features = rules
            .get("features")
            .and_then(serde_json::Value::as_array)
            .ok_or("the rules member has no features")?
            .iter()
            .filter_map(|f| {
                let s = |k: &str| f.get(k).and_then(serde_json::Value::as_str);
                Some((
                    s("name")?.to_owned(),
                    s("about").unwrap_or_default().to_owned(),
                ))
            })
            .collect();
        let formulas = rules
            .get("formulas")
            .and_then(serde_json::Value::as_array)
            .ok_or("the rules member has no formulas")?
            .iter()
            .filter_map(|f| f.as_str().map(str::to_owned))
            .collect();
        let choices = rules
            .get("choices")
            .and_then(serde_json::Value::as_object)
            .into_iter()
            .flatten()
            .map(|(field, names)| {
                let names = names
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|n| n.as_str().map(str::to_owned))
                    .collect();
                (field.clone(), names)
            })
            .collect();
        Ok(Some(Self {
            profiles,
            features,
            formulas,
            choices,
        }))
    }

    /// Every rule the row can say, by its field's name (`features.ratings`), in the tab's order:
    /// the row's own fields, the systems, the formula settings, then the server properties the
    /// profile or the world sets.
    #[must_use]
    pub fn fields(&self, profile: &str, world: Option<&ShownItem>) -> Vec<String> {
        let mut out: Vec<String> = FIELDS.iter().map(|f| (*f).to_owned()).collect();
        out.extend(self.features.iter().map(|(n, _)| format!("features.{n}")));
        out.extend(self.formulas.iter().map(|n| format!("formulas.{n}")));
        let mut properties: Vec<String> = self
            .profiles
            .get(profile)
            .into_iter()
            .flat_map(|p| p.keys())
            .chain(
                world
                    .into_iter()
                    .flat_map(|w| w.fields.iter().map(|f| &f.name)),
            )
            .filter(|n| n.starts_with("properties."))
            .cloned()
            .collect();
        properties.sort();
        properties.dedup();
        out.extend(properties);
        out
    }

    /// What system `name` is.
    #[must_use]
    pub fn about(&self, field: &str) -> Option<&str> {
        let name = field.strip_prefix("features.")?;
        self.features
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, a)| a.as_str())
    }
}

/// One rule as the tab shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownRule {
    /// The row's field (`max_level`, `features.ratings`).
    pub field: String,
    /// The profile's value; `None` where the era says none (no cap, no letter).
    pub profile: Option<T>,
    /// The world's value at the layer, and the layer that set it; `None`, the profile's.
    pub set: Option<(T, String)>,
}

impl ShownRule {
    /// The value the world plays.
    #[must_use]
    pub fn value(&self) -> Option<&T> {
        self.set.as_ref().map(|(v, _)| v).or(self.profile.as_ref())
    }
}

/// Every rule of the world `world` (its rules item at the layer, `None` when no layer writes one)
/// playing the era `profile`.
#[must_use]
pub fn shown(vocab: &RulesVocabulary, profile: &str, world: Option<&ShownItem>) -> Vec<ShownRule> {
    let era = vocab.profiles.get(profile);
    vocab
        .fields(profile, world)
        .into_iter()
        .map(|field| ShownRule {
            profile: era.and_then(|p| p.get(&field)).cloned(),
            set: world
                .and_then(|w| w.fields.iter().find(|f| f.name == field))
                .map(|f| (f.value.clone(), f.layer.clone())),
            field,
        })
        .collect()
}

/// A change of one rule.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// Set to this value.
    To(T),
    /// Back to the profile's: set by no layer.
    Profile,
    /// Back to what the world below the layer has.
    Below,
}

/// The layer's rules row once `field` is changed as `change` says, over `below` (the rules as the
/// world below the layer has them, `None` when no layer below writes them) and `layer` (the
/// layer's row now): only what differs from below, and the fields below that the layer unsets.
/// `None` when the layer then says nothing.
#[must_use]
pub fn changed(
    schema: &RowSchema,
    below: Option<&toml::Table>,
    layer: Option<&toml::Table>,
    field: &str,
    change: &Change,
) -> Option<toml::Table> {
    let now = match layer {
        Some(l) => layer_rows::over(schema, KIND, below, l),
        None => below.cloned().unwrap_or_default(),
    };
    let mut want = layer_rows::fields(schema, KIND, &now);
    let was = below.map(|b| layer_rows::fields(schema, KIND, b));
    match change {
        Change::To(v) => {
            want.insert(field.to_owned(), v.clone());
        }
        Change::Profile => {
            want.remove(field);
        }
        Change::Below => match was.as_ref().and_then(|w| w.get(field)) {
            Some(v) => {
                want.insert(field.to_owned(), v.clone());
            }
            None => {
                want.remove(field);
            }
        },
    }
    let wanted = layer_rows::row_of(schema, KIND, &want);
    layer_rows::delta(schema, KIND, below, &wanted).filter(|r| !r.is_empty())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a world's rules shown and changed by the world editor)
    use super::*;
    use crate::repo::ShownField;

    fn schema() -> RowSchema {
        RowSchema::of_schema(&serde_json::json!({ "rows": {
            "rules": {
                "file": "rules.toml", "item": "rules", "key": [],
                "fields": { "max_level": "int", "start": "text", "creation_skills": { "either": ["text", { "list": { "name": "Skill" } }] } },
                "maps": {
                    "features": { "key": "text", "value": "bool" },
                    "formulas": { "key": "text", "value": { "either": ["bool", "int", "text"] } },
                    "properties": { "key": "text", "value": { "either": ["bool", "int", "text"] } },
                },
            },
        } }))
        .unwrap()
        .unwrap()
    }

    fn vocabulary() -> RulesVocabulary {
        let rules = serde_json::json!({
            "profiles": {
                "newdereth": {
                    "start": "chargen",
                    "creation_skills": ["Axe", "Sword"],
                    "features": { "ratings": false, "housing": true },
                    "formulas": { "vitae": "eor" },
                    "properties": { "use_wield_requirements": false },
                },
            },
            "features": [
                { "name": "ratings", "about": "Damage ratings." },
                { "name": "housing", "about": "Houses." },
            ],
            "formulas": ["vitae"],
            "choices": { "start": ["chargen", "world"], "formulas.vitae": ["eor", "before-tod"] },
        });
        RulesVocabulary::of_schema(Some(&rules), &schema())
            .unwrap()
            .unwrap()
    }

    fn item(fields: &[(&str, T, &str)]) -> ShownItem {
        ShownItem {
            kind: KIND.into(),
            fields: fields
                .iter()
                .map(|(n, v, l)| ShownField {
                    name: (*n).into(),
                    value: v.clone(),
                    layer: (*l).into(),
                    at: None,
                    prov: dereth_pack::provenance::Provenance::default(),
                })
                .collect(),
        }
    }

    /// Every rule is listed, in order, with the profile's value and, where a layer sets it, the
    /// world's value and that layer; a system says what it is.
    #[test]
    fn each_rule_shows_the_profiles_value_and_the_layer_that_sets_it() {
        let v = vocabulary();
        let world = item(&[
            ("features.ratings", T::Boolean(true), "base"),
            ("properties.xp_modifier", T::Float(2.0), "2027-01-fixes"),
        ]);
        let rules = shown(&v, "newdereth", Some(&world));
        let names: Vec<&str> = rules.iter().map(|r| r.field.as_str()).collect();
        assert_eq!(names[0], "max_level");
        assert_eq!(
            &names[FIELDS.len()..],
            [
                "features.ratings",
                "features.housing",
                "formulas.vitae",
                "properties.use_wield_requirements",
                "properties.xp_modifier",
            ]
        );
        let get = |f: &str| rules.iter().find(|r| r.field == f).unwrap();
        let ratings = get("features.ratings");
        assert_eq!(ratings.profile, Some(T::Boolean(false)));
        assert_eq!(ratings.set, Some((T::Boolean(true), "base".into())));
        assert_eq!(ratings.value(), Some(&T::Boolean(true)));
        assert_eq!(get("features.housing").value(), Some(&T::Boolean(true)));
        assert_eq!(get("features.housing").set, None);
        assert_eq!(get("max_level").value(), None, "the era sets no cap");
        assert_eq!(v.about("features.ratings"), Some("Damage ratings."));
        assert_eq!(v.choices["formulas.vitae"], ["eor", "before-tod"]);
        // An era the server doesn't know: no profile values, the world's still shown.
        assert_eq!(shown(&v, "ac2", Some(&world))[FIELDS.len()].profile, None);
    }

    /// A change is written as the layer's row of what it changes over the world below; set back to
    /// below's value it is no longer the layer's, and put back to the profile's it is unset where a
    /// layer below sets it.
    #[test]
    fn a_rules_change_is_the_layers_row_of_only_what_it_changes() {
        let s = schema();
        let below: toml::Table =
            toml::from_str("max_level = 50\n[features]\nratings = true\n").unwrap();
        let on = changed(
            &s,
            Some(&below),
            None,
            "features.housing",
            &Change::To(T::Boolean(false)),
        )
        .unwrap();
        assert_eq!(on.to_string(), "[features]\nhousing = false\n");
        // Another change keeps the first.
        let two = changed(
            &s,
            Some(&below),
            Some(&on),
            "max_level",
            &Change::To(T::Integer(60)),
        )
        .unwrap();
        assert_eq!(two["max_level"], T::Integer(60));
        assert_eq!(two["features"]["housing"], T::Boolean(false));
        // Back to below: the layer no longer says it.
        let back = changed(&s, Some(&below), Some(&two), "max_level", &Change::Below).unwrap();
        assert!(!back.contains_key("max_level"));
        assert_eq!(
            changed(
                &s,
                Some(&below),
                Some(&on),
                "features.housing",
                &Change::Below
            ),
            None
        );
        // Back to the profile, where a layer below sets it: unset.
        let profile =
            changed(&s, Some(&below), None, "features.ratings", &Change::Profile).unwrap();
        assert_eq!(
            profile["remove_fields"],
            T::Array(vec![T::String("features.ratings".into())])
        );
        // With nothing below, the profile's is no row at all.
        assert_eq!(
            changed(&s, None, Some(&on), "features.housing", &Change::Profile),
            None
        );
    }
}
