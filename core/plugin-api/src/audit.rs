//! The answers to a server's plugin audit: `Admin_QueryPluginList` (`0x02AE`) and
//! `Admin_QueryPlugin` (`0x02B1`), answered with `Admin_QueryPluginListResponse` (`0x02AF`) and
//! `Admin_QueryPluginResponse` (`0x02B2`).
//!
//! A client with no plugin host answers the list with the text retail sends when no third-party
//! interface is running, and answers a single query with failure and `Unknown` in every field.
//! A client with plugins loaded answers truthfully: every loaded plugin, and for one plugin the
//! permissions its manifest names.

/// The list answer when no plugin is loaded.
pub const NOT_IN_USE: &str = "3rd party API not in use.";

/// What an answer to a single query leaves in a field it has nothing for.
pub const UNKNOWN: &str = "Unknown";

/// One loaded plugin, as an audit answer describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub homepage: String,
    /// The permissions its manifest names, by those names. Every plugin may do what each
    /// covers; the list says what the plugin uses.
    pub permissions: Vec<String>,
}

/// The `0x02AF` list: one line per loaded plugin, `id version`, or [`NOT_IN_USE`] when there are
/// none.
#[must_use]
pub fn list_text(entries: &[AuditEntry]) -> String {
    if entries.is_empty() {
        return NOT_IN_USE.to_owned();
    }
    entries
        .iter()
        .map(|e| format!("{} {}", e.id, e.version))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `0x02B2` fields for a query naming `asked`: `(success, name, author, email, webpage)`.
///
/// A plugin is found by id or by name. Found, the name field is `name version (id)`, the author
/// field the authors, the email field (which has no use of its own here) the permissions its
/// manifest names, and the webpage field the homepage.
#[must_use]
pub fn query_answer(entries: &[AuditEntry], asked: &str) -> (bool, String, String, String, String) {
    let Some(e) = entries.iter().find(|e| e.id == asked || e.name == asked) else {
        return (
            false,
            asked.to_owned(),
            UNKNOWN.into(),
            UNKNOWN.into(),
            UNKNOWN.into(),
        );
    };
    let permissions = if e.permissions.is_empty() {
        "none".to_owned()
    } else {
        e.permissions.join(", ")
    };
    (
        true,
        format!("{} {} ({})", e.name, e.version, e.id),
        if e.authors.is_empty() {
            UNKNOWN.to_owned()
        } else {
            e.authors.join(", ")
        },
        format!("permissions: {permissions}"),
        if e.homepage.is_empty() {
            UNKNOWN.to_owned()
        } else {
            e.homepage.clone()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{list_text, query_answer, AuditEntry, NOT_IN_USE};

    fn entry() -> AuditEntry {
        AuditEntry {
            id: "example.xpmeter".into(),
            name: "XP Meter".into(),
            version: "1.2.0".into(),
            authors: vec!["A. Author".into()],
            homepage: String::new(),
            permissions: vec!["chat.print".into(), "read".into()],
        }
    }

    #[test]
    fn with_nothing_loaded_the_list_says_the_interface_is_not_in_use() {
        assert_eq!(list_text(&[]), NOT_IN_USE);
        assert_eq!(list_text(&[entry()]), "example.xpmeter 1.2.0");
    }

    #[test]
    fn a_single_query_names_the_manifests_permissions_or_fails_with_unknown() {
        let (ok, name, author, email, web) = query_answer(&[entry()], "example.xpmeter");
        assert!(ok);
        assert_eq!(name, "XP Meter 1.2.0 (example.xpmeter)");
        assert_eq!(author, "A. Author");
        assert_eq!(email, "permissions: chat.print, read");
        assert_eq!(web, "Unknown");
        let (ok, name, author, ..) = query_answer(&[entry()], "Decal");
        assert!(!ok);
        assert_eq!((name.as_str(), author.as_str()), ("Decal", "Unknown"));
    }
}
