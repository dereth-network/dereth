//! Synthetic dats and installs, and one world list's answers, for tests in this crate and the ones
//! above it.

use std::path::Path;

use crate::datset::{DatRole, Iterations};

pub use crate::dat::testdat::{build as dat_bytes, eor as eor_dat_bytes};

/// Write a Classic pair, `portal.dat` and `cell.dat`, with the given iterations.
pub fn write_classic_set(dir: &Path, portal: u32, cell: u32) {
    std::fs::create_dir_all(dir).expect("create the test folder");
    std::fs::write(
        dir.join("portal.dat"),
        crate::dat::testdat::pre_tod(false, portal),
    )
    .expect("write a test dat");
    std::fs::write(
        dir.join("cell.dat"),
        crate::dat::testdat::pre_tod(true, cell),
    )
    .expect("write a test dat");
}

/// Write a folder of the four dats with the given iteration counts. Missing counts are missing files.
pub fn write_dat_set(dir: &Path, it: Iterations) {
    std::fs::create_dir_all(dir).expect("create the test folder");
    for (role, set, sub) in [
        (DatRole::Portal, 1, 0),
        (DatRole::Cell, 2, 1),
        (DatRole::Local, 3, 1),
        (DatRole::Highres, 1, 0x6946_6948),
    ] {
        if let Some(n) = it.get(role) {
            std::fs::write(
                dir.join(role.file_name()),
                crate::dat::testdat::eor(set, sub, n),
            )
            .expect("write a test dat");
        }
    }
}

/// One world list as every launcher reads it: what its sources answer, and the worlds a launcher
/// shows from those answers. Each launcher's own test reads it through that launcher and compares
/// what it shows with [`from_the_hub`] and [`from_the_community`].
pub mod world_list {
    use crate::serverlist::{COMMUNITY_LIST_URL, HUB_LIST_URL};
    use crate::world::{Emulator, World};

    /// When the list is read, in seconds since the epoch: 2026-10-11T18:00:00Z.
    pub const READ_AT: u64 = 1_791_741_600;

    /// The hub's list, one page: two ACE worlds whose heartbeats say they are up (twelve on) and
    /// starting (none on), and a world the launcher's table knows by its list id, whose heartbeat
    /// says it is down. Every heartbeat counts until 2099.
    #[must_use]
    pub fn hub_page() -> String {
        let row = |name: &str,
                   family: &str,
                   address: &str,
                   port: u16,
                   id: &str,
                   state: &str,
                   players: Option<u32>| {
            serde_json::json!({
                "slug": crate::serverlist::slug_for(name), "name": name, "isDemo": false,
                "emulator": {"family": family},
                "endpoint": {"mode": "manual", "address": address, "port": port},
                "provenance": {"externalId": id, "legacyType": "PvE"},
                "status": {"state": state, "playersOnline": players,
                           "source": "server-heartbeat", "expiresAt": "2099-01-01T00:00:00.000Z"},
            })
        };
        serde_json::json!({"data": [
            row("Coldeve", "ACE", "192.0.2.10", 9000, "6b1f8d6e-0c55-4c1a-9d0e-31a2c7d0a001", "online", Some(12)),
            row("Frostfell", "ACE", "192.0.2.11", 9010, "6b1f8d6e-0c55-4c1a-9d0e-31a2c7d0a002", "maintenance", Some(0)),
            row("Seedsow", "GDL", "192.0.2.12", 9050, "394C58D0-885D-466B-B17F-D7E0B96FE3E2", "offline", None),
        ], "nextOffset": null})
        .to_string()
    }

    /// The community's own list: a plain ACE world, and the world the launcher's table knows.
    pub const COMMUNITY: &str = r"<?xml version='1.0' encoding='utf-8'?>
<ArrayOfServerItem>
  <ServerItem>
    <id>9d17ce44-7db5-40c1-b7bb-a01c9de21d00</id><name>AChard</name>
    <description>ACE EOR PVP</description><emu>ACE</emu>
    <server_host>192.0.2.20</server_host><server_port>9000</server_port>
    <type>PvP</type><status>Stable</status>
  </ServerItem>
  <ServerItem>
    <id>394C58D0-885D-466B-B17F-D7E0B96FE3E2</id><name>Seedsow</name><emu>GDL</emu>
    <server_host>192.0.2.12</server_host><server_port>9050</server_port>
  </ServerItem>
</ArrayOfServerItem>";

    /// What fetching `url` brings: the hub's page when `hub_answers`, else a failure; the
    /// community's list; and nothing from anywhere else.
    pub fn fetch(
        hub_answers: bool,
    ) -> impl FnMut(&str) -> Result<Vec<u8>, String> + Send + 'static {
        move |url: &str| {
            if url.starts_with(HUB_LIST_URL) {
                if hub_answers {
                    Ok(hub_page().into_bytes())
                } else {
                    Err(format!("{url}: HTTP 502"))
                }
            } else if url == COMMUNITY_LIST_URL {
                Ok(COMMUNITY.as_bytes().to_vec())
            } else {
                Err(format!("{url}: not found"))
            }
        }
    }

    /// A world as a launcher shows it, in brief: its key, its name, the server software it runs, the
    /// era it plays (none: the end of retail), the address it is reached at, and how many are on.
    pub type Summary = (
        String,
        String,
        Emulator,
        Option<String>,
        Option<String>,
        Option<u32>,
    );

    /// `w` in brief.
    #[must_use]
    pub fn summary(w: &World) -> Summary {
        (
            w.slug.clone(),
            w.name.clone(),
            w.emulator,
            w.era.clone(),
            w.endpoint
                .as_ref()
                .map(|e| format!("{}:{}", e.address, e.port)),
            w.players,
        )
    }

    fn of(
        slug: &str,
        name: &str,
        emulator: Emulator,
        era: Option<&str>,
        address: &str,
        players: Option<u32>,
    ) -> Summary {
        (
            slug.to_owned(),
            name.to_owned(),
            emulator,
            era.map(str::to_owned),
            Some(address.to_owned()),
            players,
        )
    }

    /// The worlds a launcher shows from the hub's list, with how many are on as the hub has it.
    #[must_use]
    pub fn from_the_hub() -> Vec<Summary> {
        vec![
            of(
                "coldeve",
                "Coldeve",
                Emulator::Ace,
                None,
                "192.0.2.10:9000",
                Some(12),
            ),
            of(
                "frostfell",
                "Frostfell",
                Emulator::Ace,
                None,
                "192.0.2.11:9010",
                Some(0),
            ),
            of(
                "seedsow",
                "Seedsow",
                Emulator::Gdle,
                Some("infiltration"),
                "192.0.2.12:9050",
                None,
            ),
        ]
    }

    /// The worlds a launcher shows from the community's list, when the hub does not answer.
    #[must_use]
    pub fn from_the_community() -> Vec<Summary> {
        vec![
            of(
                "achard",
                "AChard",
                Emulator::Ace,
                None,
                "192.0.2.20:9000",
                None,
            ),
            of(
                "seedsow",
                "Seedsow",
                Emulator::Gdle,
                Some("infiltration"),
                "192.0.2.12:9050",
                None,
            ),
        ]
    }
}
