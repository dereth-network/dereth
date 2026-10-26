//! A world's policy document: the small TOML file a world publishes to say which kinds of plugin
//! automation it allows (`docs/plugins/policy.md`).
//!
//! ```toml
//! policy_version = 1
//! world = "Example World"
//!
//! [automation]          # per domain: "allow" | "deny" | "interactive-only"
//! loot = "allow"
//! combat = "deny"
//! session = "allow"     # logging in and out, choosing a character
//! default = "deny"      # a domain not named (deny when absent)
//!
//! [limits]
//! peers_local = 3       # other clients on the machine a plugin may coordinate (0 = none)
//! pk_automation = false # automation off while the character is a player killer
//! read_parity = false   # plugins see only what the radar shows
//!
//! [notes]
//! text = "Unattended macroing is not allowed."
//! url = "https://example.org/rules"
//! ```
//!
//! A world publishes it inline in its world entry (the `plugin_policy` field) or at an HTTPS URL
//! the entry names. Keys a later version adds are ignored; a known key of the wrong kind, or an
//! unknown rule, refuses the document (the client then applies the policy of a world that
//! publishes none, and says why).

use std::collections::BTreeMap;

use dereth_plugin_api::{AutoDomain, DomainRule, WorldPolicy};

/// The document versions this client reads.
pub const VERSION: i64 = 1;

/// Read a policy document.
///
/// # Errors
/// Not TOML, a version this client does not read, or a rule or limit of the wrong kind.
pub fn parse(text: &str) -> Result<WorldPolicy, String> {
    let doc: toml::Table =
        toml::from_str(text).map_err(|e| format!("not a policy document: {e}"))?;
    let version = doc
        .get("policy_version")
        .and_then(toml::Value::as_integer)
        .ok_or("policy_version is missing")?;
    if version != VERSION {
        return Err(format!(
            "policy_version {version} is not one this client reads"
        ));
    }
    let rule = |key: &str, v: &toml::Value| -> Result<DomainRule, String> {
        v.as_str().and_then(DomainRule::parse).ok_or_else(|| {
            format!("automation.{key} must be \"allow\", \"deny\" or \"interactive-only\"")
        })
    };
    let mut automation = BTreeMap::new();
    let mut policy = WorldPolicy::published(BTreeMap::new());
    if let Some(a) = doc.get("automation") {
        let a = a.as_table().ok_or("automation must be a table")?;
        for (k, v) in a {
            match k.as_str() {
                "default" => policy.default = rule(k, v)?,
                "session" => policy.session = rule(k, v)?,
                d => {
                    if let Some(d) = AutoDomain::parse(d) {
                        automation.insert(d, rule(k, v)?);
                    }
                }
            }
        }
    }
    policy.automation = automation;
    policy.world = doc
        .get("world")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    if let Some(l) = doc.get("limits") {
        let l = l.as_table().ok_or("limits must be a table")?;
        if let Some(n) = l.get("peers_local") {
            policy.peers_local = Some(
                n.as_integer()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or("limits.peers_local must be a whole number")?,
            );
        }
        let flag = |k: &str| -> Result<Option<bool>, String> {
            l.get(k)
                .map(|v| {
                    v.as_bool()
                        .ok_or(format!("limits.{k} must be true or false"))
                })
                .transpose()
        };
        if let Some(b) = flag("pk_automation")? {
            policy.pk_automation = b;
        }
        if let Some(b) = flag("read_parity")? {
            policy.read_parity = b;
        }
    }
    if let Some(n) = doc.get("notes").and_then(toml::Value::as_table) {
        policy.notes = n
            .get("text")
            .and_then(toml::Value::as_str)
            .map(str::to_owned);
        policy.notes_url = n
            .get("url")
            .and_then(toml::Value::as_str)
            .map(str::to_owned);
    }
    Ok(policy)
}

#[cfg(test)]
mod tests {
    use super::parse;
    use dereth_plugin_api::{AutoDomain, DomainRule};

    #[test]
    fn a_published_document_names_its_rules_and_denies_what_it_leaves_out() {
        let p = parse(
            "policy_version = 1\nworld = \"Example\"\n[automation]\nloot = \"allow\"\n\
             heal = \"interactive-only\"\nsession = \"allow\"\nflying = \"allow\"\n\
             [limits]\npeers_local = 2\n[notes]\ntext = \"Be nice.\"\n",
        )
        .expect("valid");
        assert!(p.published);
        assert_eq!(p.rule(AutoDomain::Loot), DomainRule::Allow);
        assert_eq!(p.rule(AutoDomain::Heal), DomainRule::InteractiveOnly);
        assert_eq!(p.rule(AutoDomain::Combat), DomainRule::Deny);
        assert!(p.allows_session(false));
        assert_eq!(p.peers_local, Some(2));
        assert_eq!(p.notes.as_deref(), Some("Be nice."));
        assert_eq!(p.world.as_deref(), Some("Example"));
    }

    #[test]
    fn a_wrong_version_or_rule_refuses_the_document() {
        assert!(parse("policy_version = 2").is_err());
        assert!(parse("world = \"x\"").is_err());
        assert!(parse("policy_version = 1\n[automation]\nloot = \"sometimes\"").is_err());
        assert!(parse("policy_version = 1\n[limits]\npeers_local = -1").is_err());
    }
}
