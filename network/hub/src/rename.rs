//! Renaming a world: its slug changes, and everything the hub keeps of it follows.
//!
//! The registry keeps a world under an id, and most of what belongs to it (its key, claims,
//! verifications, reports, log, Discord mirrors and rooms) under that id, so those follow at once.
//! What is kept by slug follows in the same transaction: the world's registration, its characters'
//! links to persons, their link codes and the log of them, the directory's listings and holds, the
//! character each person speaks as there, and every person friend, handed-back friend or address of
//! one of its characters (`Bob@old` becomes `Bob@new`). A community listing stays matched to its
//! entry by the entry's id, so the next import updates it under its new slug.
//!
//! A serving hub reads the registry as it goes: within a second of the rename it closes a link
//! still open under the old slug, as for any world it no longer has, and the world links again
//! once its server names the new slug. The channels are the one thing a serving hub keeps in
//! memory and writes back, so a world whose characters are in a channel is not renamed.

use dereth_network_protocol::address::is_network_slug;
use rusqlite::{params, Transaction, TransactionBehavior};
use serde::Serialize;
use serde_json::json;

use crate::registry::{is_registry_slug, Registry, RegistryError};

type Result<T> = std::result::Result<T, RegistryError>;

/// The columns that hold a world's slug.
const WORLD_COLUMNS: &[&str] = &[
    "directory_listings",
    "directory_holds",
    "person_characters",
    "person_codes",
    "person_returns",
    "person_identities",
];

/// The columns that hold a character's address, or its identity key, as `name@world`.
const ADDRESS_COLUMNS: &[(&str, &str)] = &[
    ("directory_listings", "address_key"),
    ("directory_listings", "address"),
    ("directory_listings", "patron_key"),
    ("directory_listings", "monarch_key"),
    ("directory_holds", "address_key"),
    ("directory_holds", "address"),
    ("person_characters", "address_key"),
    ("person_characters", "address"),
    ("person_codes", "address_key"),
    ("person_codes", "address"),
    ("person_code_log", "address_key"),
    ("person_friends", "handle_key"),
    ("person_friends", "handle"),
    ("person_friends", "added_from"),
    ("person_friends", "added_from_address"),
    ("person_returns", "address_key"),
    ("person_returns", "handle_key"),
    ("person_returns", "handle"),
    ("person_identities", "address_key"),
];

/// A world renamed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Renamed {
    pub from: String,
    pub to: String,
    /// Whether the world has a key, which still works under its new slug.
    #[serde(rename = "hasKey")]
    pub has_key: bool,
}

/// One line of a `fix-slugs` plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlugFix {
    pub slug: String,
    /// `rename`, `keep` (claimed or managed: listed, not touched) or `conflict`.
    pub action: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Registry {
    /// Renames the world `old` to `new`, carrying across everything the hub keeps of it, in one
    /// transaction; its log, and its operator's, say so. Its key keeps working.
    ///
    /// # Errors
    /// 400 for a `new` that is no network slug; 404; 409 for a taken slug, the same slug, or a
    /// world whose characters are in a channel.
    pub fn rename(&self, old: &str, new: &str) -> Result<Renamed> {
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let renamed = self.rename_within(old, new, "admin")?;
        tx.commit()?;
        Ok(renamed)
    }

    /// Renames, in one transaction, each unclaimed community listing whose slug is no
    /// network slug to the slug the import gives a new listing of its name; claimed and managed
    /// worlds with such slugs are listed and kept. A dry run (`apply` false) changes nothing.
    ///
    /// # Errors
    /// Database errors only.
    pub fn fix_slugs(&self, apply: bool) -> Result<Vec<SlugFix>> {
        let tx = Transaction::new_unchecked(
            &self.db,
            if apply {
                TransactionBehavior::Immediate
            } else {
                TransactionBehavior::Deferred
            },
        )?;
        let mut plan = Vec::new();
        for row in self.all_rows()? {
            if is_network_slug(&row.slug) {
                continue;
            }
            let external = row.registry["provenance"]["externalId"]
                .as_str()
                .map(str::to_owned);
            let claims: i64 = self.db.query_row(
                "SELECT count(*) FROM verifications WHERE server_id = ? AND state = 'open'",
                [&row.id],
                |r| r.get(0),
            )?;
            let unclaimed = row.state() == "unclaimed"
                && row.operator_id.is_none()
                && row.key_hash.is_none()
                && claims == 0;
            let Some(external) = external.filter(|_| unclaimed) else {
                plan.push(SlugFix {
                    slug: row.slug.clone(),
                    action: "keep",
                    to: None,
                    reason: Some(
                        if unclaimed {
                            "not a community listing"
                        } else {
                            "claimed, being claimed or managed: rename it with `rename`"
                        }
                        .into(),
                    ),
                });
                continue;
            };
            let choices = crate::community::slug_choices(&row.metadata.name, &external);
            let mut fix = SlugFix {
                slug: row.slug.clone(),
                action: "conflict",
                to: None,
                reason: Some("every slug its name gives is taken".into()),
            };
            for choice in choices {
                if self.row(&choice)?.is_some() {
                    continue;
                }
                match self.rename_within(&row.slug, &choice, "fix-slugs") {
                    Ok(_) => {
                        fix.action = "rename";
                        fix.to = Some(choice);
                        fix.reason = None;
                    }
                    Err(e) if e.status == 500 => return Err(e),
                    Err(e) => fix.reason = Some(e.message),
                }
                break;
            }
            plan.push(fix);
        }
        if apply {
            tx.commit()?;
        } else {
            tx.rollback()?;
        }
        Ok(plan)
    }

    /// [`Self::rename`], within the caller's transaction, `by` naming what asked for it in the log.
    fn rename_within(&self, old: &str, new: &str, by: &str) -> Result<Renamed> {
        if !is_network_slug(new) || !is_registry_slug(new) {
            return Err(RegistryError::bad(
                "A network slug is 2-12 of a-z and 0-9, with single hyphens between them, and not reserved",
            ));
        }
        let row = self.row(old)?.ok_or_else(RegistryError::not_found)?;
        if row.slug == new {
            return Err(RegistryError::conflict("The world has this slug already"));
        }
        if self.row(new)?.is_some() {
            return Err(RegistryError::conflict("That slug is taken"));
        }
        let old = row.slug.clone();
        let at_old = format!("@{old}");
        let at_new = format!("@{new}");
        let in_channels: Vec<String> = {
            let mut q = self
                .db
                .prepare("SELECT name FROM hub_channels WHERE instr(value, ?) > 0 ORDER BY name")?;
            let names = q
                .query_map([format!("{at_old}\"")], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            names
        };
        if !in_channels.is_empty() {
            return Err(RegistryError::conflict(format!(
                "The world's characters are in channels the serving hub keeps in memory ({}): they leave those channels first",
                in_channels.join(", ")
            )));
        }
        // A character's identity key moves with the link it belongs to: the checks wait for the
        // commit.
        self.db.execute_batch("PRAGMA defer_foreign_keys = ON")?;
        self.db.execute(
            "UPDATE servers SET slug = ?1, metadata = json_set(metadata, '$.slug', ?1) WHERE id = ?2",
            params![new, row.id],
        )?;
        for table in WORLD_COLUMNS {
            self.db.execute(
                &format!("UPDATE OR REPLACE {table} SET world = ?1 WHERE world = ?2"),
                params![new, old],
            )?;
        }
        for (table, column) in ADDRESS_COLUMNS {
            self.db.execute(
                &format!(
                    "UPDATE OR REPLACE {table} SET {column} = substr({column}, 1, length({column}) - length(?1)) || ?2
                     WHERE substr({column}, -length(?1)) = ?1"
                ),
                params![at_old, at_new],
            )?;
        }
        // A listed character's profile names its patron and monarch by address.
        self.db.execute(
            "UPDATE directory_listings SET profile = replace(profile, ?1, ?2) WHERE world = ?3",
            params![format!("{at_old}\""), format!("{at_new}\""), new],
        )?;
        let details = json!({ "from": old, "to": new, "by": by });
        self.event(&row.id, "renamed", &details)?;
        if let Some(op) = &row.operator_id {
            self.operator_event(op, "renamed", &details)?;
        }
        Ok(Renamed {
            from: old,
            to: new.to_owned(),
            has_key: row.key_hash.is_some(),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use crate::registry::{CommunityEntry, Registration, Registry};

    fn registry() -> Registry {
        Registry::open(
            ":memory:",
            "dereth.network",
            Box::new(std::time::SystemTime::now),
        )
        .expect("a registry")
    }

    fn registration(slug: &str, name: &str) -> Registration {
        Registration::from_json(&json!({
            "slug": slug, "name": name, "description": "A world.", "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
            "emulator": { "family": "ACE" },
        }))
        .expect("a registration")
    }

    /// A community entry as an import before network slugs made it: under `slug`, whatever its
    /// name folds to.
    fn listing(n: u32, slug: &str, name: &str, hash: &str) -> CommunityEntry {
        CommunityEntry {
            external_id: format!("{n:08x}-0000-4000-8000-000000000000"),
            registration: registration(slug, name),
            source_hash: hash.into(),
            legacy_type: "PK".into(),
        }
    }

    fn kinds(audit: &Value) -> Vec<String> {
        audit
            .as_array()
            .expect("events")
            .iter()
            .map(|e| e["kind"].as_str().expect("a kind").to_owned())
            .collect()
    }

    fn slugs(r: &Registry) -> Vec<String> {
        r.all_rows()
            .expect("rows")
            .into_iter()
            .map(|r| r.slug)
            .collect()
    }

    const EVIDENCE: &str = "Saw the world's admin panel on a call with its operator.";

    /// Spec: hub: `rename` gives a claimed world its new slug and keeps its claim, its proof, its
    /// key, its operator and its log; the old slug is free and its key no longer answers to it.
    #[test]
    fn a_renamed_world_keeps_its_claim_proof_key_and_log() {
        let mut r = registry();
        r.import_community(&[listing(1, "hollowtide-pk", "Hollowtide PK", "h1")], true)
            .expect("imported");
        let (_, key) = r.claim("hollowtide-pk", EVIDENCE).expect("claimed");
        let id = r.row("hollowtide-pk").expect("read").expect("there").id;
        r.db.execute_batch(
            "INSERT INTO operators(id, discord_id, name, created_at)
               VALUES ('op1', '100', 'Hollow', '2026-01-01T00:00:00.000Z');",
        )
        .expect("an operator");
        r.db.execute("UPDATE servers SET operator_id = 'op1' WHERE id = ?", [&id])
            .expect("owned");
        r.db.execute(
            "INSERT INTO verifications(id, server_id, operator_id, key_hash, token_hash,
               token_expires_at, created_at, state)
             VALUES ('v1', ?, 'op1', NULL, 'x', '2026-01-02T00:00:00.000Z',
               '2026-01-01T00:00:00.000Z', 'verified')",
            [&id],
        )
        .expect("a proof");

        let renamed = r.rename("hollowtide-pk", "hollowtide").expect("renamed");
        assert_eq!(
            (renamed.from.as_str(), renamed.to.as_str(), renamed.has_key),
            ("hollowtide-pk", "hollowtide", true)
        );
        let row = r.row("hollowtide").expect("read").expect("there");
        assert_eq!(row.id, id, "the same world");
        assert_eq!(row.metadata.slug, "hollowtide", "its registration too");
        assert_eq!(row.state(), "managed");
        assert_eq!(row.operator_id.as_deref(), Some("op1"));
        assert_eq!(
            row.registry["provenance"]["externalId"],
            "00000001-0000-4000-8000-000000000000"
        );
        assert!(r.row("hollowtide-pk").expect("read").is_none());
        assert!(r.authenticate("hollowtide", &key), "the key still works");
        assert!(!r.authenticate("hollowtide-pk", &key));
        let proofs: i64 =
            r.db.query_row(
                "SELECT count(*) FROM verifications WHERE server_id = ? AND state = 'verified'",
                [&id],
                |q| q.get(0),
            )
            .expect("counted");
        assert_eq!(proofs, 1);
        let log = kinds(&r.audit("hollowtide").expect("its log"));
        assert_eq!(
            log,
            [
                "renamed",
                "key-rotated",
                "ownership-reviewed",
                "community-imported"
            ],
            "its log, the rename newest"
        );
        let operator_log: String = r
            .db
            .query_row(
                "SELECT details FROM operator_events WHERE operator_id = 'op1' AND kind = 'renamed'",
                [],
                |q| q.get(0),
            )
            .expect("its operator's log says so");
        assert!(operator_log.contains("hollowtide-pk"), "{operator_log}");
    }

    /// Spec: hub: `rename` refuses a slug that is no network slug, one that is taken, and a world
    /// the registry does not have, and changes nothing.
    #[test]
    fn rename_refuses_a_bad_or_taken_slug() {
        let r = registry();
        r.register(&registration("coldeve", "Coldeve"))
            .expect("registered");
        r.register(&registration("frost", "Frost"))
            .expect("registered");
        for bad in [
            "a-very-long-one",
            "darktide",
            "world",
            "api",
            "Upper",
            "x",
            "two--dashes",
            "-edge",
        ] {
            let e = r.rename("coldeve", bad).expect_err(bad);
            assert_eq!(e.status, 400, "{bad}");
        }
        assert_eq!(r.rename("coldeve", "frost").expect_err("taken").status, 409);
        assert_eq!(
            r.rename("coldeve", "coldeve").expect_err("same").status,
            409
        );
        assert_eq!(
            r.rename("nowhere", "elsewhere").expect_err("none").status,
            404
        );
        assert_eq!(slugs(&r), ["coldeve", "frost"]);
    }

    /// Spec: hub: a rename carries the world's characters across: their links to persons, the
    /// character each speaks as, their directory listings, and every person friend naming one.
    #[test]
    fn a_rename_carries_the_worlds_characters_across() {
        use dereth_network_protocol::{Address, Federation, Profile};
        let r = registry();
        r.register(&registration("aa", "World aa"))
            .expect("registered");
        r.register(&registration("bb", "World bb"))
            .expect("registered");
        r.set_federation("aa", Federation::On).expect("on");
        let bob = Address::parse("Bob@aa").expect("an address");
        let profile = Profile {
            level: 10,
            heritage: 1,
            gender: 2,
            ..Profile::default()
        };
        r.listing(&bob, Some(&profile.into())).expect("listed");
        r.db.execute_batch(
            "INSERT INTO operators(id, discord_id, name, created_at)
               VALUES ('p1', '200', 'Person', '2026-01-01T00:00:00.000Z');
             INSERT INTO person_characters(address_key, address, world, person_id, linked_at)
               VALUES ('bob@aa', 'Bob@aa', 'aa', 'p1', '2026-01-01T00:00:00.000Z'),
                      ('ann@bb', 'Ann@bb', 'bb', 'p1', '2026-01-01T00:00:00.000Z');
             INSERT INTO person_identities(person_id, world, address_key, set_at)
               VALUES ('p1', 'aa', 'bob@aa', '2026-01-01T00:00:00.000Z');
             INSERT INTO person_friends(person_id, handle_key, handle, added_from,
               added_from_address, added_at)
               VALUES ('p1', 'cat@aa', 'Cat@aa', 'ann@bb', 'Ann@bb', '2026-01-01T00:00:00.000Z');",
        )
        .expect("linked");

        r.rename("aa", "cc").expect("renamed");
        let one = |sql: &str| -> String { r.db.query_row(sql, [], |q| q.get(0)).expect(sql) };
        assert_eq!(
            one("SELECT address || ' ' || world FROM person_characters WHERE world != 'bb'"),
            "Bob@cc cc"
        );
        assert_eq!(
            one("SELECT world || ' ' || address_key FROM person_identities"),
            "cc bob@cc"
        );
        assert_eq!(
            one("SELECT handle || ' ' || handle_key || ' ' || added_from FROM person_friends"),
            "Cat@cc cat@cc ann@bb",
            "a friend on the renamed world, added from another"
        );
        assert_eq!(
            one("SELECT address FROM person_characters WHERE world = 'bb'"),
            "Ann@bb",
            "another world's characters stay as they are"
        );
        assert!(r.directory_character("Bob@cc").expect("asked").is_some());
        assert!(r.directory_character("Bob@aa").expect("asked").is_none());
    }

    /// Spec: hub: a world whose characters are in a channel is not renamed, since a serving hub
    /// keeps its channels in memory.
    #[test]
    fn a_world_whose_characters_are_in_a_channel_is_not_renamed() {
        let r = registry();
        r.register(&registration("aa", "World aa"))
            .expect("registered");
        r.set_channel(
            "oldguard",
            Some(r#"{"owner":"Bob@aa","members":{"bob@aa":"Bob@aa"},"invites":[]}"#),
        )
        .expect("a channel");
        let e = r.rename("aa", "cc").expect_err("refused");
        assert_eq!(e.status, 409);
        assert!(e.message.contains("oldguard"), "{}", e.message);
        assert_eq!(slugs(&r), ["aa"]);
    }

    /// Spec: hub: after a rename the next community import matches the listing by its entry's id:
    /// it updates the listing under its new slug and never makes the old one again.
    #[test]
    fn a_reimport_after_a_rename_does_not_recreate_the_old_slug() {
        let mut r = registry();
        r.import_community(&[listing(1, "hollowtide-pk", "Hollowtide PK", "h1")], true)
            .expect("imported");
        r.rename("hollowtide-pk", "hollowtide").expect("renamed");
        for (hash, action) in [("h1", "unchanged"), ("h2", "update")] {
            let plan = r
                .import_community(&[listing(1, "hollowtide-pk", "Hollowtide PK", hash)], true)
                .expect("imported again");
            assert_eq!(
                (plan[0].slug.as_str(), plan[0].action.as_str()),
                ("hollowtide", action)
            );
            assert_eq!(slugs(&r), ["hollowtide"]);
        }
        let row = r.row("hollowtide").expect("read").expect("there");
        assert_eq!(
            row.metadata.slug, "hollowtide",
            "an update keeps the new slug"
        );
    }

    /// Spec: hub: `fix-slugs` renames only unclaimed community listings whose slug is no network
    /// slug, to the slug the import gives their name; claimed worlds are listed and kept, and a dry
    /// run changes nothing.
    #[test]
    fn fix_slugs_renames_only_unclaimed_long_slugs() {
        let mut r = registry();
        let entries = [
            listing(1, "hollowtide-pk", "Hollowtide PK", "h1"),
            listing(2, "hollowtide-pve", "Hollowtide PvE", "h2"),
            listing(3, "frostfire-legends", "Frostfire Legends", "h3"),
            listing(4, "coldeve", "Coldeve", "h4"),
        ];
        r.import_community(&entries, true).expect("imported");
        r.claim("frostfire-legends", EVIDENCE).expect("claimed");
        let before = slugs(&r);

        let plan = r.fix_slugs(false).expect("a dry run");
        assert_eq!(slugs(&r), before, "a dry run changes nothing");
        let line = |slug: &str| plan.iter().find(|f| f.slug == slug).expect(slug).clone();
        assert_eq!(
            (line("hollowtide-pk").action, line("hollowtide-pk").to),
            ("rename", Some("hollowtide".to_owned()))
        );
        assert_eq!(
            (line("hollowtide-pve").action, line("hollowtide-pve").to),
            ("rename", Some("hollowtide-2".to_owned()))
        );
        assert_eq!(line("frostfire-legends").action, "keep");
        assert_eq!(line("frostfire-legends").to, None);
        assert!(
            plan.iter().all(|f| f.slug != "coldeve"),
            "a network slug is left alone"
        );

        assert_eq!(r.fix_slugs(true).expect("applied"), plan);
        assert_eq!(
            slugs(&r),
            ["coldeve", "frostfire-legends", "hollowtide", "hollowtide-2"]
        );
        let log = kinds(&r.audit("hollowtide").expect("its log"));
        assert!(log.iter().any(|k| k == "renamed"), "{log:?}");
        let again = r.import_community(&entries, true).expect("imported again");
        assert!(
            again
                .iter()
                .all(|p| p.action == "unchanged" || p.action == "skip"),
            "{again:?}"
        );
        assert_eq!(slugs(&r).len(), 4, "nothing made again");
    }
}
