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
    /// starting (none on), a world the launcher's table knows by its list id, whose heartbeat
    /// says it is down, and an Empyrean world linked to the hub, up (three on), which the hub
    /// names a WebSocket endpoint and a status document for. One ACE world's row names a plain
    /// `ws://` endpoint on another machine, which no launcher follows. Every heartbeat counts
    /// until 2099.
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
        let mut frostfell = row(
            "Frostfell",
            "ACE",
            "192.0.2.11",
            9010,
            "6b1f8d6e-0c55-4c1a-9d0e-31a2c7d0a002",
            "maintenance",
            Some(0),
        );
        frostfell["websocketUrl"] = serde_json::json!("ws://192.0.2.11:9180/");
        let mut ironwood = row(
            "Ironwood",
            "Empyrean",
            "192.0.2.13",
            9060,
            "6b1f8d6e-0c55-4c1a-9d0e-31a2c7d0a004",
            "online",
            Some(3),
        );
        ironwood["websocketUrl"] = serde_json::json!(IRONWOOD_WEBSOCKET);
        ironwood["statusUrl"] = serde_json::json!(IRONWOOD_STATUS);
        serde_json::json!({"data": [
            row("Coldeve", "ACE", "192.0.2.10", 9000, "6b1f8d6e-0c55-4c1a-9d0e-31a2c7d0a001", "online", Some(12)),
            frostfell,
            row("Seedsow", "GDL", "192.0.2.12", 9050, "394C58D0-885D-466B-B17F-D7E0B96FE3E2", "offline", None),
            ironwood,
        ], "nextOffset": null})
        .to_string()
    }

    /// The WebSocket endpoint the hub names for Ironwood, the world linked to it.
    pub const IRONWOOD_WEBSOCKET: &str = "wss://ironwood.example.org/ws";

    /// Ironwood's status document, as the hub names it.
    pub const IRONWOOD_STATUS: &str = "https://ironwood.example.org/status";

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
    /// era it plays (none: the end of retail), the address it is reached at, how many are on, and
    /// the WebSocket endpoint a browser reaches it at directly (none: through the relay).
    pub type Summary = (
        String,
        String,
        Emulator,
        Option<String>,
        Option<String>,
        Option<u32>,
        Option<String>,
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
            w.websocket_url.clone(),
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
            None,
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
            reached(
                IRONWOOD_WEBSOCKET,
                of(
                    "ironwood",
                    "Ironwood",
                    Emulator::Empyrean,
                    None,
                    "192.0.2.13:9060",
                    Some(3),
                ),
            ),
        ]
    }

    /// `s`, reached directly at the WebSocket endpoint `websocket`.
    fn reached(websocket: &str, s: Summary) -> Summary {
        let (slug, name, emulator, era, address, players, _) = s;
        let websocket = Some(websocket.to_owned());
        (slug, name, emulator, era, address, players, websocket)
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

/// A plugin's zip: `uses` is its manifest's tail (its permissions and web domains).
#[must_use]
pub fn plugin_zip(id: &str, version: &str, api: &str, uses: &str) -> Vec<u8> {
    let manifest = format!(
        "id = \"{id}\"\nname = \"Meter\"\nversion = \"{version}\"\napi = \"{api}\"\n{uses}"
    );
    dereth_plugin_package::Package {
        origin: String::new(),
        files: [
            ("plugin.toml".to_owned(), manifest.into_bytes()),
            ("main.lua".to_owned(), b"print(1)".to_vec()),
        ]
        .into(),
    }
    .to_zip()
}

/// A source's `plugins.json` listing `zips`, each at `<id>.zip` beside the list.
#[must_use]
pub fn plugin_list(zips: &[&[u8]]) -> Vec<u8> {
    use dereth_plugin_package::catalog::{Catalog, Entry};
    let mut c = Catalog::default();
    for z in zips {
        let probe = Entry::for_zip(z, "x.zip", "", "").expect("a plugin's zip");
        let e = Entry::for_zip(z, &format!("{}.zip", probe.id), "Notes.", "2026-10-26")
            .expect("a plugin's zip");
        c.put(e);
    }
    c.to_json().into_bytes()
}

/// Write a plugin's folder at `dir`, as an author keeps it on disk: its `plugin.toml`, for this
/// client's API version, and its script.
pub fn write_plugin_folder(dir: &Path, id: &str, name: &str, version: &str) {
    std::fs::create_dir_all(dir).expect("create the plugin's folder");
    let manifest = format!(
        "id = \"{id}\"\nname = \"{name}\"\nversion = \"{version}\"\napi = \"{}\"\nentry = \"main.lua\"\ndescription = \"Says hi\"\n",
        dereth_plugin_api::ApiVersion::CURRENT
    );
    std::fs::write(dir.join("plugin.toml"), manifest).expect("write its manifest");
    std::fs::write(dir.join("main.lua"), "print(\"hi\")\n").expect("write its script");
}
