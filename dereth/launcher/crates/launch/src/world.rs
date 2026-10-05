//! A world: one registry entry, and everything the launcher needs to know to put a player in it.
//!
//! The directory's `/v1/servers` row is the starting point. Schema 2 grows it with the facts the
//! pre-launch check needs (which clients the world accepts, which dats it expects, whether it
//! patches them) and the ones the interface needs (how accounts are made, where the rules are).
//! Every new field is optional, so every row published today is still a valid world, and an absent
//! field means **"we have not been told"**, never "no". The check treats the two differently.
//!
//! Reading is tolerant in the way `/v1/servers` reading always was: a row without a slug is
//! skipped, a field of the wrong type is treated as absent, and nothing in one row can cost the
//! player another.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::datset::Iterations;

/// The server software a world runs. It decides the command-line form a retail client needs and
/// which status source is worth asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Emulator {
    Empyrean,
    Ace,
    /// ClassicACE, a fork of ACE.
    ClassicAce,
    Gdle,
    /// Not said, or software the launcher does not know by name, which it treats alike. A record
    /// saved as `other` reads as this.
    #[default]
    #[serde(alias = "other")]
    Unknown,
}

impl Emulator {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "empyrean" => Emulator::Empyrean,
            "ace" => Emulator::Ace,
            "classicace" | "classic ace" | "classic-ace" | "classic_ace" => Emulator::ClassicAce,
            // The community list spells it `GDL`.
            "gdle" | "gdl" => Emulator::Gdle,
            _ => Emulator::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Emulator::Empyrean => "Empyrean",
            Emulator::Ace => "ACE",
            Emulator::ClassicAce => "ClassicACE",
            Emulator::Gdle => "GDLE",
            Emulator::Unknown => "Unknown",
        }
    }

    /// Whether this server is known to insist on the end-of-retail wire protocol. Every emulator
    /// named here compares the logon version string exactly: `"1802"`, except ClassicACE, whose
    /// rulesets each want their own ([`World::logon_version`]).
    pub fn requires_end_of_retail_protocol(self) -> bool {
        matches!(
            self,
            Emulator::Empyrean | Emulator::Ace | Emulator::ClassicAce | Emulator::Gdle
        )
    }
}

/// Whether a world is up, as last reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldState {
    Online,
    /// Up, but at or near its population ceiling.
    High,
    Offline,
    /// Starting up: reachable, not yet letting anyone in.
    Starting,
    /// Nobody has said. Most of the directory looks like this, and it is still worth trying.
    #[default]
    Unknown,
}

impl WorldState {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "online" | "up" => WorldState::Online,
            "high" | "busy" | "full" => WorldState::High,
            "offline" | "down" | "closed" => WorldState::Offline,
            "starting" => WorldState::Starting,
            _ => WorldState::Unknown,
        }
    }

    /// A login is worth attempting. Status can be stale, so only an explicit "down" says no.
    pub fn is_joinable(self) -> bool {
        !matches!(self, WorldState::Offline)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub address: String,
    pub port: u16,
    pub transport: Option<String>,
}

/// How a world creates accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountModel {
    /// The first login under a new name creates the account. ACE's default, and the reason the
    /// launcher warns before a first sign-in: a typo becomes a new account, silently.
    AutoCreateOnFirstLogin,
    WebSignup,
    Invite,
    #[default]
    Unknown,
}

impl AccountModel {
    pub fn parse(s: &str) -> Self {
        match s {
            "auto_create_on_first_login" => AccountModel::AutoCreateOnFirstLogin,
            "web_signup" => AccountModel::WebSignup,
            "invite" => AccountModel::Invite,
            _ => AccountModel::Unknown,
        }
    }
}

/// Where a world's live status comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusMethod {
    /// Empyrean's public status document.
    EmpyreanHttp,
    /// The directory's own heartbeat, as carried in the row.
    RegistryHeartbeat,
    /// An unauthenticated login packet; any reply means up.
    UdpProbe,
    #[default]
    None,
}

impl StatusMethod {
    pub fn parse(s: &str) -> Self {
        match s {
            "empyrean_http" => StatusMethod::EmpyreanHttp,
            "registry_heartbeat" => StatusMethod::RegistryHeartbeat,
            "udp_probe" => StatusMethod::UdpProbe,
            _ => StatusMethod::None,
        }
    }
}

/// A world's own data files, for a world that ships them. Always a link: the launcher never hosts
/// or fetches-and-runs them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomDats {
    pub url: String,
    pub sha256: Option<String>,
    pub size: Option<u64>,
    pub iterations: Iterations,
    pub license_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorldDats {
    /// What the world's own dats report. `None` means the world has not said.
    pub expected: Option<Iterations>,
    pub patches_over_wire: Option<bool>,
    pub custom: Option<CustomDats>,
    pub highres_required: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Links {
    pub website: Option<String>,
    pub discord: Option<String>,
    pub rules: Option<String>,
    pub guide: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Operator {
    pub name: Option<String>,
    pub contact: Option<String>,
    pub key_id: Option<String>,
}

/// Who said a world's era, or the systems it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Told {
    /// The world: its status document or its directory row.
    World,
    /// The player, for a world that does not say.
    Player,
}

/// One world.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct World {
    /// Stable identity. Every per-world record the launcher keeps is keyed by this.
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub ruleset: Option<String>,
    /// The era the world plays (`"eor"`, the end of retail, or an earlier one such as
    /// `"infiltration"`), from the registry row or, when it answers, the world's status document.
    /// `None` when neither says.
    #[serde(default)]
    pub era: Option<String>,
    /// The systems the world has (`ratings=false,trade=true,...`), from its status document when
    /// it answers; each one over the era's table. `None` when it does not say.
    #[serde(default)]
    pub era_features: Option<String>,
    /// Who named [`World::era`]; `None` when nobody has.
    #[serde(default)]
    pub era_source: Option<Told>,
    /// Who named [`World::era_features`]; `None` when nobody has.
    #[serde(default)]
    pub features_source: Option<Told>,
    pub emulator: Emulator,
    /// The server software's version, when the world says it (Empyrean's status document, or what
    /// a server reported to the directory).
    #[serde(default)]
    pub emulator_version: Option<String>,
    /// The community list's id for the world, which never changes.
    #[serde(default)]
    pub list_id: Option<String>,
    /// How settled the world says it is: `Stable`, `Development` or `Experimental`.
    #[serde(default)]
    pub development_status: Option<String>,
    pub endpoint: Option<Endpoint>,
    /// Client ids the world accepts. Empty means "the end-of-retail wire protocol"; see
    /// [`World::accepts`].
    pub accepted_clients: Vec<String>,
    pub preferred_client: Option<String>,
    pub dats: WorldDats,
    pub account_model: AccountModel,
    pub signup_url: Option<String>,
    pub reset_url: Option<String>,
    pub status_method: StatusMethod,
    pub status_url: Option<String>,
    pub state: WorldState,
    pub players: Option<u32>,
    pub links: Links,
    pub operator: Option<Operator>,
    pub updated_at: Option<String>,
    /// The row's schema. Rows from before schema 2 say nothing and read as 1.
    pub schema: u32,
    /// The logon version string the world's server accepts, when it is not the end of retail's
    /// `"1802"` or is known to be it ([`crate::known`]). `None`: not said.
    #[serde(default)]
    pub logon_version: Option<String>,
    /// The client rules the world plays by, as `--world-profile` names them. `None`: the end of
    /// retail's.
    #[serde(default)]
    pub world_profile: Option<String>,
    /// The data set the world is drawn from (`modern` or `classic`), when it is not the one its
    /// era suggests. `None`: the era's.
    #[serde(default)]
    pub world_base: Option<String>,
}

/// The logon version string every emulator requires today.
pub const END_OF_RETAIL_NET_VERSION: &str = "1802";

impl World {
    /// A bare world, for tests and for `servers.txt` rows.
    pub fn new(slug: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            slug: slug.into(),
            name: name.into(),
            schema: 1,
            ..Self::default()
        }
    }

    /// Whether a client with this id and logon version would be let in.
    ///
    /// An explicit list is the world's word and is taken as is. An empty list means the
    /// end-of-retail protocol: the Dereth client, and any retail build whose logon version is
    /// `"1802"`.
    pub fn accepts(&self, client_id: &str, net_version: Option<&str>) -> bool {
        if !self.accepted_clients.is_empty() {
            // `dereth` in a list means the Dereth client at any version; a versioned entry means
            // that version only.
            return self
                .accepted_clients
                .iter()
                .any(|c| c == client_id || (c == "dereth" && client_id.starts_with("dereth-")));
        }
        // The Dereth client sends whatever logon version the world wants; a retail build sends
        // its own, which must be that one.
        let wanted = self
            .logon_version
            .as_deref()
            .unwrap_or(END_OF_RETAIL_NET_VERSION);
        client_id == "dereth"
            || client_id.starts_with("dereth-")
            || (client_id.starts_with("acclient-") && net_version == Some(wanted))
    }

    /// Whether the world writes to the player's dats. Unknown counts as no: every emulator ships
    /// with patching off.
    pub fn patches(&self) -> bool {
        self.dats.patches_over_wire == Some(true)
    }

    /// Whether this world needs a dat set of its own rather than the shared end-of-retail one.
    pub fn needs_private_dats(&self) -> bool {
        self.patches() || self.dats.custom.is_some()
    }

    pub fn is_playable(&self) -> bool {
        self.state.is_joinable() && self.endpoint.is_some()
    }
}

fn str_at<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut v = v;
    for k in path {
        v = v.get(k)?;
    }
    v.as_str().filter(|s| !s.is_empty())
}

fn string_at(v: &Value, path: &[&str]) -> Option<String> {
    str_at(v, path).map(str::to_owned)
}

fn u64_at(v: &Value, path: &[&str]) -> Option<u64> {
    let mut v = v;
    for k in path {
        v = v.get(k)?;
    }
    v.as_u64()
}

fn u32_of(v: Option<&Value>) -> Option<u32> {
    v?.as_u64().and_then(|n| u32::try_from(n).ok())
}

/// Read a `{portal, cell, local, highres}` object. Any subset of the four may be present.
pub fn parse_iterations(v: &Value) -> Option<Iterations> {
    if !v.is_object() {
        return None;
    }
    let it = Iterations {
        portal: u32_of(v.get("portal")),
        cell: u32_of(v.get("cell")),
        local: u32_of(v.get("local")),
        highres: u32_of(v.get("highres")),
    };
    (!it.is_empty()).then_some(it)
}

/// Read one directory row. `None` for a row with no slug: it cannot be remembered or reconciled
/// across a refresh, so it is skipped rather than given an invented identity.
pub fn parse_world(row: &Value) -> Option<World> {
    let slug = str_at(row, &["slug"])?.to_owned();
    let mut w = World::new(slug.clone(), str_at(row, &["name"]).unwrap_or(&slug));
    w.schema = u64_at(row, &["schema"])
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(1);
    w.description = string_at(row, &["description"]);
    w.ruleset = string_at(row, &["ruleset"]);
    w.era = string_at(row, &["era"]);
    w.era_source = w.era.is_some().then_some(Told::World);
    // The directory spells the emulator as a string, or as `{"family": ...}` with `software` too.
    w.emulator = str_at(row, &["emulator"])
        .or_else(|| str_at(row, &["emulator", "family"]))
        .or_else(|| str_at(row, &["software"]))
        .map(Emulator::parse)
        .unwrap_or_default();
    w.emulator_version = string_at(row, &["emulator", "version"])
        .or_else(|| string_at(row, &["status", "reported", "emulatorVersion"]));
    w.list_id = string_at(row, &["provenance", "externalId"]);
    w.development_status = string_at(row, &["maturity"]);

    // `endpoint` is where the client connects; the directory's own `fqdn`/`port` fill gaps only.
    let host = str_at(row, &["endpoint", "address"]).or_else(|| str_at(row, &["fqdn"]));
    let port = u64_at(row, &["endpoint", "port"])
        .or_else(|| u64_at(row, &["port"]))
        .filter(|p| *p > 0)
        .and_then(|p| u16::try_from(p).ok());
    if let (Some(address), Some(port)) = (host, port) {
        w.endpoint = Some(Endpoint {
            address: address.to_owned(),
            port,
            transport: string_at(row, &["endpoint", "transport"]),
        });
    }

    if let Some(list) = row
        .get("clients")
        .and_then(|c| c.get("accepted"))
        .and_then(Value::as_array)
    {
        w.accepted_clients = list
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::to_owned)
            .collect();
    }
    w.preferred_client = string_at(row, &["clients", "preferred"]);

    if let Some(d) = row.get("dats") {
        w.dats.expected = d.get("expected").and_then(parse_iterations);
        w.dats.patches_over_wire = d.get("patches_over_wire").and_then(Value::as_bool);
        w.dats.highres_required = d.get("highres_required").and_then(Value::as_bool);
        if let Some(c) = d.get("custom").filter(|c| c.is_object()) {
            if let Some(url) = string_at(c, &["url"]) {
                w.dats.custom = Some(CustomDats {
                    url,
                    sha256: string_at(c, &["sha256"]).map(|s| s.to_ascii_lowercase()),
                    size: u64_at(c, &["size"]),
                    iterations: c
                        .get("iterations")
                        .and_then(parse_iterations)
                        .unwrap_or_default(),
                    license_note: string_at(c, &["license_note"]),
                });
            }
        }
    }

    w.account_model = str_at(row, &["accounts", "model"])
        .map(AccountModel::parse)
        .unwrap_or_default();
    w.signup_url = string_at(row, &["accounts", "signup_url"]);
    w.reset_url = string_at(row, &["accounts", "reset_url"]);

    w.status_method = str_at(row, &["status", "method"])
        .map(StatusMethod::parse)
        .unwrap_or_default();
    w.status_url = string_at(row, &["status", "url"]);
    w.state = str_at(row, &["status", "state"])
        .map(WorldState::parse)
        .unwrap_or_default();
    w.players =
        u64_at(row, &["status", "playersOnline"]).map(|n| u32::try_from(n).unwrap_or(u32::MAX));

    w.links = Links {
        website: string_at(row, &["links", "website"])
            .or_else(|| string_at(row, &["website"]))
            .or_else(|| string_at(row, &["website_url"])),
        discord: string_at(row, &["links", "discord"])
            .or_else(|| string_at(row, &["discord"]))
            .or_else(|| string_at(row, &["discord_url"])),
        rules: string_at(row, &["links", "rules"]),
        guide: string_at(row, &["links", "guide"]),
    };
    if let Some(o) = row.get("operator").filter(|o| o.is_object()) {
        w.operator = Some(Operator {
            name: string_at(o, &["name"]),
            contact: string_at(o, &["contact"]),
            key_id: string_at(o, &["key_id"]),
        });
    }
    w.updated_at = string_at(row, &["updated_at"]);
    // A world the launcher knows by its list id: what its row cannot say.
    crate::known::apply(&mut w);
    Some(w)
}

/// Why a directory reply could not be read at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedDirectory(pub String);

impl core::fmt::Display for MalformedDirectory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "server list unreadable: {}", self.0)
    }
}

/// Read one page of the directory: its worlds, and the offset of the next page if there is one.
///
/// Accepts the paged envelope (`{"data": [...], "nextOffset": n}`), a `{"servers": [...]}` object,
/// and a bare array, which is the shape of a static `worlds.json` snapshot.
pub fn parse_page(body: &[u8]) -> Result<(Vec<World>, Option<u64>), MalformedDirectory> {
    let json: Value =
        serde_json::from_slice(body).map_err(|e| MalformedDirectory(e.to_string()))?;
    let rows = json
        .get("data")
        .or_else(|| json.get("servers"))
        .or_else(|| json.get("worlds"))
        .unwrap_or(&json)
        .as_array()
        .ok_or_else(|| MalformedDirectory("no array of servers".into()))?;
    let worlds = rows.iter().filter_map(parse_world).collect();
    Ok((worlds, json.get("nextOffset").and_then(Value::as_u64)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spec's example schema-2 row.
    const EULMORE: &str = r#"{
      "schema": 2, "slug": "eulmore", "name": "Eulmore",
      "description": "Internal dereth.network testbed world.", "ruleset": "PvE",
      "emulator": "empyrean",
      "endpoint": { "address": "eulmore.etheirys.network", "port": 19000, "transport": "udp" },
      "clients": { "accepted": ["dereth", "acclient-6096", "acclient-4186"], "preferred": "dereth" },
      "dats": {
        "expected": { "portal": 2072, "cell": 982, "local": 994, "highres": 497 },
        "patches_over_wire": false, "highres_required": true, "custom": null
      },
      "accounts": { "model": "auto_create_on_first_login", "signup_url": null, "reset_url": null },
      "status": { "method": "empyrean_http", "url": "https://eulmore.etheirys.network/v1/world" },
      "links": { "website": "https://dereth.network", "discord": null },
      "operator": { "name": "dereth.network", "key_id": "ed25519:abc" },
      "updated_at": "2026-09-26T00:00:00Z"
    }"#;

    /// A row in the shape `/v1/servers` returns today.
    const TODAY: &str = r#"{"slug":"achard","name":"AChard","description":"ACE EOR PVP Server",
        "endpoint":{"address":"a-chard.ddns.net","port":9000},"fqdn":"achard.dereth.network",
        "status":{"state":"unknown","playersOnline":null}}"#;

    fn world(s: &str) -> World {
        parse_world(&serde_json::from_str(s).unwrap()).unwrap()
    }

    #[test]
    fn a_schema_2_row_reads_completely() {
        let w = world(EULMORE);
        assert_eq!(w.schema, 2);
        assert_eq!(w.emulator, Emulator::Empyrean);
        assert_eq!(w.endpoint.as_ref().unwrap().port, 19000);
        assert_eq!(
            w.accepted_clients,
            ["dereth", "acclient-6096", "acclient-4186"]
        );
        assert_eq!(w.preferred_client.as_deref(), Some("dereth"));
        assert_eq!(w.dats.expected, Some(Iterations::END_OF_RETAIL));
        assert_eq!(w.dats.patches_over_wire, Some(false));
        assert_eq!(w.dats.highres_required, Some(true));
        assert_eq!(w.dats.custom, None);
        assert_eq!(w.account_model, AccountModel::AutoCreateOnFirstLogin);
        assert_eq!(w.status_method, StatusMethod::EmpyreanHttp);
        assert_eq!(w.operator.unwrap().key_id.as_deref(), Some("ed25519:abc"));
        assert_eq!(w.era, None, "a row that names no era");
        let era = world(&EULMORE.replacen(
            "\"ruleset\": \"PvE\",",
            "\"ruleset\": \"PvE\", \"era\": \"infiltration\",",
            1,
        ));
        assert_eq!(era.era.as_deref(), Some("infiltration"));
    }

    #[test]
    fn todays_rows_are_still_worlds_with_nothing_claimed() {
        let w = world(TODAY);
        assert_eq!(w.schema, 1);
        assert_eq!(w.emulator, Emulator::Unknown);
        assert_eq!(w.dats.expected, None, "not told is not end of retail");
        assert_eq!(w.dats.patches_over_wire, None);
        assert_eq!(w.state, WorldState::Unknown);
        assert_eq!(w.players, None);
        assert!(w.is_playable(), "unknown status is still worth trying");
    }

    #[test]
    fn an_empty_accept_list_means_the_end_of_retail_protocol() {
        let w = world(TODAY);
        assert!(w.accepts("dereth", Some("1802")));
        assert!(w.accepts("dereth-0.4.0", None));
        assert!(w.accepts("acclient-6096", Some("1802")));
        assert!(
            !w.accepts("acclient-4079", Some("2011")),
            "a different logon version is refused"
        );
        assert!(
            !w.accepts("acclient-4079", None),
            "an unknown logon version is not assumed"
        );
    }

    #[test]
    fn an_explicit_accept_list_is_the_worlds_word() {
        let w = world(EULMORE);
        assert!(w.accepts("acclient-6096", Some("1802")));
        assert!(
            !w.accepts("acclient-6067", Some("1802")),
            "not listed, so not accepted"
        );
        assert!(
            w.accepts("dereth-0.4.0", Some("1802")),
            "`dereth` covers every version of it"
        );
    }

    #[test]
    fn a_world_that_wants_another_logon_takes_the_dereth_client_and_only_that_retail_build() {
        let mut w = world(TODAY);
        w.logon_version = Some("c118".into());
        assert!(
            w.accepts("dereth", None),
            "the Dereth client sends what is wanted"
        );
        assert!(!w.accepts("acclient-6096", Some("1802")));
        assert!(w.accepts("acclient-6096", Some("c118")));
    }

    #[test]
    fn a_custom_dat_world_needs_its_own_set() {
        let w = world(
            r#"{"slug":"frostfell","dats":{"custom":{"url":"https://example/f.zip",
            "sha256":"ABCD","iterations":{"portal":2090,"cell":982,"local":994,"highres":497}}}}"#,
        );
        let c = w.dats.custom.as_ref().unwrap();
        assert_eq!(c.iterations.portal, Some(2090));
        assert_eq!(
            c.sha256.as_deref(),
            Some("abcd"),
            "hashes compare lower-case"
        );
        assert!(w.needs_private_dats());
    }

    #[test]
    fn a_patching_world_needs_its_own_set_and_a_silent_one_does_not() {
        assert!(world(r#"{"slug":"a","dats":{"patches_over_wire":true}}"#).needs_private_dats());
        assert!(!world(r#"{"slug":"a","dats":{}}"#).needs_private_dats());
        assert!(!world(TODAY).needs_private_dats());
    }

    #[test]
    fn a_field_of_the_wrong_type_is_absent_not_fatal() {
        let w = world(
            r#"{"slug":"a","emulator":7,"clients":{"accepted":"dereth"},
            "dats":{"expected":"2072","patches_over_wire":"yes"},"endpoint":{"address":"h","port":99999}}"#,
        );
        assert_eq!(w.emulator, Emulator::Unknown);
        assert!(w.accepted_clients.is_empty());
        assert_eq!(w.dats.expected, None);
        assert_eq!(w.dats.patches_over_wire, None);
        assert_eq!(w.endpoint, None, "a port that does not fit is no endpoint");
    }

    /// A row as `/v1/servers` returns it now: the emulator as an object, the links at the top, and
    /// what the server reported under its status.
    #[test]
    fn todays_directory_row_names_its_emulator_version_links_and_list_id() {
        let w = world(
            r#"{"slug":"eulmore","name":"Eulmore","maturity":"Stable","emulator":{"family":"ACE"},
            "website":"https://w.example","discord":"https://d.example",
            "provenance":{"source":"community-list","externalId":"abc"},
            "status":{"state":"stale","reported":{"emulatorVersion":"1.77.4778"}}}"#,
        );
        assert_eq!(w.emulator, Emulator::Ace);
        assert_eq!(w.emulator_version.as_deref(), Some("1.77.4778"));
        assert_eq!(w.list_id.as_deref(), Some("abc"));
        assert_eq!(w.development_status.as_deref(), Some("Stable"));
        assert_eq!(w.links.website.as_deref(), Some("https://w.example"));
        assert_eq!(w.links.discord.as_deref(), Some("https://d.example"));
        assert_eq!(
            world(r#"{"slug":"a","software":"GDL"}"#).emulator,
            Emulator::Gdle
        );
        assert_eq!(w.era_source, None);
        assert_eq!(Emulator::parse("ClassicACE"), Emulator::ClassicAce);
        assert_eq!(Emulator::ClassicAce.label(), "ClassicACE");
        assert_eq!(
            Emulator::parse("Thwarg"),
            Emulator::Unknown,
            "a name not known is unknown"
        );
        let saved: Emulator = serde_json::from_str("\"other\"").unwrap();
        assert_eq!(
            saved,
            Emulator::Unknown,
            "a record saved as other reads as unknown"
        );
        assert_eq!(
            world(r#"{"slug":"a","era":"eor"}"#).era_source,
            Some(Told::World)
        );
    }

    #[test]
    fn pages_bare_arrays_and_community_list_links_all_read() {
        let (w, next) =
            parse_page(br#"{"nextOffset":50,"data":[{"slug":"a"},{"name":"no slug"}]}"#).unwrap();
        assert_eq!((w.len(), next), (1, Some(50)));
        let (w, _) = parse_page(br#"[{"slug":"a","website_url":"https://a.example"}]"#).unwrap();
        assert_eq!(w[0].links.website.as_deref(), Some("https://a.example"));
        assert!(parse_page(b"{}").is_err());
        assert!(parse_page(b"nonsense").is_err());
    }
}
