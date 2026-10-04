// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/MasterConfiguration.cs
//! Not ACE: `empyrean.toml`, the server's configuration file.
//!
//! It is the one source of the server's settings: [`MasterConfiguration`] with readable snake_case
//! keys and sections. One schema table ([`SECTIONS`]) maps every TOML key to the serde name the
//! configuration types carry (ACE's `Config.js` name, for the keys ACE has):
//!
//! * the reader renames a TOML document's keys to those names, keeping their order, and then
//!   deserialises it with the configuration types' serde code (defaults for keys left out, a
//!   section present replacing the whole default object, numbers readable from strings);
//! * the writer emits every key with a comment giving its default and the ACE setting it stands
//!   for;
//! * the converter ([`from_config_js_str`]) reads ACE's `Config.js` once, lists every key it drops,
//!   and hands the configuration to the writer.
//!
//! Unknown keys and sections are ignored, and each is reported so the caller can warn (a misspelt
//! key in a hand-written file would otherwise pass silently). ACE settings Empyrean no longer has
//! ([`REMOVED`]) and the renamed `[mysql]` section are ignored and reported with the reason, so an
//! older file still loads. A key Empyrean renames ([`RENAMED`]) is read under its old name, with a
//! warning, for the one release that renamed it. [`warnings`] gives the start-up lines for all of
//! these.

use serde_json::Value as Json;
use toml::{Table, Value};

use crate::config_manager::{ConfigError, ConfigManager};
use crate::master_configuration::MasterConfiguration;

/// One key: its TOML name, the serde name it stands for, and what it does.
#[derive(Debug, Clone, Copy)]
pub struct Key {
    /// The TOML key (snake_case).
    pub toml: &'static str,
    /// The serde name: ACE's `Config.js` key, or Empyrean's own name for a key ACE lacks.
    pub ace: &'static str,
    /// Whether ACE has this setting (false: an Empyrean key).
    pub in_ace: bool,
    /// What the key does, one comment line per `\n`.
    pub help: &'static str,
    /// The value the writer shows, commented out, for a key that is not set.
    pub unset: &'static str,
}

/// One TOML table: its dotted path, the serde object it stands for, and its keys.
#[derive(Debug, Clone, Copy)]
pub struct Section {
    /// The TOML path (`["server", "network"]`).
    pub toml: &'static [&'static str],
    /// The serde (ACE `Config.js`) path (`["Server", "Network"]`).
    pub ace: &'static [&'static str],
    /// An array of tables (`[[server.preloaded_landblocks]]`) rather than a table.
    pub array: bool,
    /// What the section is for.
    pub help: &'static str,
    /// The keys, in the order the writer emits them.
    pub keys: &'static [Key],
}

/// An ACE key.
const fn k(toml: &'static str, ace: &'static str, help: &'static str) -> Key {
    Key {
        toml,
        ace,
        in_ace: true,
        help,
        unset: "\"\"",
    }
}

/// An Empyrean key (ACE has no such setting).
const fn e(toml: &'static str, ace: &'static str, help: &'static str) -> Key {
    Key {
        toml,
        ace,
        in_ace: false,
        help,
        unset: "\"\"",
    }
}

/// An `[era]` system key: on or off over the profile's value, which it has when not set. The
/// writer shows it commented out at its end-of-retail value.
const fn system(toml: &'static str, ace: &'static str, help: &'static str) -> Key {
    Key {
        toml,
        ace,
        in_ace: false,
        help,
        unset: "true",
    }
}

/// [`system`] for a system the end of retail lacks.
const fn system_off(toml: &'static str, ace: &'static str, help: &'static str) -> Key {
    Key {
        unset: "false",
        ..system(toml, ace, help)
    }
}

/// The schema: every section and key of `empyrean.toml`, in the order the writer emits them.
pub const SECTIONS: &[Section] = &[
    Section {
        toml: &["server"],
        ace: &["Server"],
        array: false,
        help: "The game server.",
        keys: &[
            k("world_name", "WorldName", "The world's name."),
            k(
                "dat_files_directory",
                "DatFilesDirectory",
                "The folder that holds the retail dat files (client_cell_1.dat, client_portal.dat,\n\
                 client_highres.dat, client_local_English.dat). Empty means this file's folder, then the\n\
                 folder the server executable is in. Only this key names the folder: the server reads no\n\
                 environment variable.",
            ),
            e(
                "character_screen_message",
                "CharacterScreenMessage",
                "Text for the character screen's message box, sent with the character list at\n\
                 log-in. Only clients of the 2005 era show it. Empty sends nothing. Use \\n for a new\n\
                 line.",
            ),
            e(
                "world_pack_path",
                "WorldPackPath",
                "The world database, built from ACE's world-database SQL dump with\n\
                 `empyrean-import --sql <dump.sql> --out world.pack`. It replaces ACE's MySql.World connection.\n\
                 The default (./world.pack) is also looked for beside the server executable; a path you\n\
                 write names one file.",
            ),
            e(
                "world_overlay_path",
                "WorldOverlayPath",
                "The content overlay, an SQLite file beside world.pack that the developer\n\
                 content commands (import-sql, import-json, createinst, nudge, ...) write to and the world\n\
                 database reads first. Empty means no overlay. `empyrean-import --overlay <file>` publishes it\n\
                 into a new world.pack. Needs world_base_sql (and world_base_patches) to write.",
            ),
            e(
                "world_base_sql",
                "WorldBaseSql",
                "The ACE world-database SQL dump world.pack was built from. The overlay\n\
                 loads it on its first write, so a write runs as an empyrean-import patch does.",
            ),
            e(
                "world_base_patches",
                "WorldBasePatches",
                "The empyrean-import patch inputs world.pack was built from, in order after the\n\
                 dump: a file or folder of SQL, or \"json:<path>\" for ACE JSON content.",
            ),
            k(
                "shutdown_interval",
                "ShutdownInterval",
                "Seconds the server waits before shutting down after a shutdown is called from the\n\
                 console or by an admin in game.",
            ),
            k(
                "server_performance_monitor_auto_start",
                "ServerPerformanceMonitorAutoStart",
                "Start the performance monitor (the /serverperformance command) with the server.",
            ),
            k(
                "shard_player_biota_cache_time",
                "ShardPlayerBiotaCacheTime",
                "Minutes a player object read from the shard database stays in memory.",
            ),
            k(
                "shard_non_player_biota_cache_time",
                "ShardNonPlayerBiotaCacheTime",
                "Minutes a non-player object read from the shard database stays in memory.",
            ),
            k(
                "landblock_preloading",
                "LandblockPreloading",
                "Load the landblocks in [[server.preloaded_landblocks]] at startup. Perma-load\n\
                 landblocks can hold server-wide mechanics and should always be active.",
            ),
            e(
                "log_level",
                "LogLevel",
                "The server log's level: \"error\", \"warn\", \"info\", \"debug\" or \"trace\". Any other\n\
                 value is warned about at startup and logs at \"info\".",
            ),
            e(
                "status_address",
                "StatusAddress",
                "The HTTP status endpoint (GET /status and GET /health) as \"<ip>:<port>\", for\n\
                 example \"127.0.0.1:9100\". Empty leaves it off. `--status <ip>:<port>` on the command\n\
                 line wins. It has no authentication: keep it on loopback or a private network.",
            ),
            e(
                "interactive_console",
                "InteractiveConsole",
                "Read console commands from standard input. Set it false when the server runs as a\n\
                 service or in a container with no terminal.",
            ),
            e(
                "source_url",
                "SourceUrl",
                "Where players are told this server's source code is (the login welcome, @source, the\n\
                 version report and the status endpoint). Empty means the repository the build came\n\
                 from. The server is AGPL-3.0: if you run a modified version for players, set this to\n\
                 where your modified source can be had.",
            ),
            e(
                "update",
                "Update",
                "Which new releases the server installs by itself: \"off\", \"patch\" (patch releases of\n\
                 this minor version, which change no database, world pack or configuration) or \"minor\"\n\
                 (also minor releases, with their database migrations and world-pack rebuilds). Major\n\
                 releases and pre-releases are only reported. `empyrean-server update --check` says what\n\
                 would be taken and why. See SETUP.md, \"Automatic updates\".",
            ),
            e(
                "update_check_hours",
                "UpdateCheckHours",
                "Hours between checks for a new release while update is not \"off\".",
            ),
            e(
                "update_warning_seconds",
                "UpdateWarningSeconds",
                "Seconds of in-game shutdown countdown before the server restarts into a new release.",
            ),
            e(
                "update_source",
                "UpdateSource",
                "Where new releases are looked for. Empty means the GitHub repository this build came\n\
                 from (the source `empyrean-server --version` prints). Otherwise a GitHub repository\n\
                 (\"https://github.com/<owner>/<name>\"), or a mirror that serves GitHub's releases API\n\
                 for one (\"https://mirror.example.org/repos/<owner>/<name>\"). Every download is\n\
                 checked against the release's SHA256SUMS and release.json.",
            ),
        ],
    },
    Section {
        toml: &["server", "network"],
        ace: &["Server", "Network"],
        array: false,
        help: "The listener and session limits.",
        keys: &[
            k(
                "host",
                "Host",
                "The address to listen on. Change it only for special network setups, such as several\n\
                 network adapters.",
            ),
            k(
                "port",
                "Port",
                "The first of two UDP ports: the server also opens port + 1. Open both in the firewall.",
            ),
            k("maximum_allowed_sessions", "MaximumAllowedSessions", "Must be above 0 for anyone to connect."),
            k("default_session_timeout", "DefaultSessionTimeout", "Seconds until an idle session is declared dead."),
            k(
                "maximum_allowed_sessions_per_ip_address",
                "MaximumAllowedSessionsPerIPAddress",
                "Sessions allowed from one IP address; -1 is unlimited.",
            ),
            k(
                "allow_unlimited_sessions_from_ip_addresses",
                "AllowUnlimitedSessionsFromIPAddresses",
                "IP addresses exempt from that limit; best kept to admins and developers.\n\
                 Example: [\"127.0.0.1\", \"8.8.8.8\"]",
            ),
        ],
    },
    Section {
        toml: &["server", "websocket"],
        ace: &["Server", "WebSocket"],
        array: false,
        help: "The WebSocket endpoint: browser clients connect here directly, one game datagram per message.\n\
               Off unless enabled. wss:// needs a certificate and key; plain ws:// is served only on a\n\
               loopback address or with behind_tls_proxy.",
        keys: &[
            e("enabled", "Enabled", "Listen for WebSocket connections."),
            e(
                "listen",
                "Listen",
                "The address to listen on, \"<ip>:<port>\". Behind a reverse proxy on this machine, a loopback\n\
                 address such as \"127.0.0.1:9180\".",
            ),
            e(
                "tls_certificate",
                "TlsCertificate",
                "The PEM certificate chain for wss://. Empty serves plain ws://, which is allowed only on\n\
                 loopback or with behind_tls_proxy.",
            ),
            e("tls_private_key", "TlsPrivateKey", "The PEM private key for tls_certificate."),
            e(
                "behind_tls_proxy",
                "BehindTlsProxy",
                "A reverse proxy in front (nginx, Cloudflare) terminates TLS, so plain ws:// may be served\n\
                 on a non-loopback address. The proxy must be the only way in.",
            ),
            e(
                "trusted_proxies",
                "TrustedProxies",
                "The proxies whose X-Forwarded-For header names the client, for the limits and the log.\n\
                 Example: [\"127.0.0.1\"]. A connection from any other address is taken as the client.",
            ),
            e(
                "allowed_origins",
                "AllowedOrigins",
                "The web pages allowed to connect, by origin. Example: [\"https://play.example.org\"].\n\
                 Empty refuses every browser page.",
            ),
            e(
                "maximum_connections_per_ip_address",
                "MaximumConnectionsPerIPAddress",
                "WebSocket connections open at once from one client address; -1 is unlimited.",
            ),
            e(
                "idle_timeout",
                "IdleTimeout",
                "Seconds a connection may send nothing before it is closed. Keep a proxy's read timeout\n\
                 longer than this.",
            ),
            e(
                "public_url",
                "PublicUrl",
                "The URL clients use, reported by the status endpoint (\"wss://play.example.org/ws\").\n\
                 Empty derives it from listen.",
            ),
        ],
    },
    Section {
        toml: &["server", "accounts"],
        ace: &["Server", "Accounts"],
        array: false,
        help: "Account creation and permissions.",
        keys: &[
            k(
                "override_character_permissions",
                "OverrideCharacterPermissions",
                "Account-level permissions (true) rather than character-level ones (retail).",
            ),
            k("default_access_level", "DefaultAccessLevel", "The access level of a new account (0 is Player)."),
            k("allow_auto_account_creation", "AllowAutoAccountCreation", "Create an account on its first login."),
            k("password_hash_work_factor", "PasswordHashWorkFactor", "The BCrypt work factor for passwords."),
            k(
                "force_work_factor_migration",
                "ForceWorkFactorMigration",
                "Rehash a password whose work factor differs from password_hash_work_factor.",
            ),
        ],
    },
    Section {
        toml: &["server", "preloaded_landblocks"],
        ace: &["Server", "PreloadedLandblocks"],
        array: true,
        help: "The landblocks loaded (and perma-loaded) at startup; they need landblock_preloading.\n\
               In most cases they need no change. Any entry written here replaces the whole default list;\n\
               `preloaded_landblocks = []` under [server] loads none.",
        keys: &[
            k("id", "Id", "The landblock id in hex."),
            k("description", "Description", "Free text."),
            k("permaload", "Permaload", "Never unload."),
            k("include_adjacents", "IncludeAdjacents", "Also load the adjacent landblocks."),
            k("enabled", "Enabled", "Use this entry."),
        ],
    },
    Section {
        toml: &["database"],
        ace: &["MySql"],
        array: false,
        help: "The database files. (ACE's MySql section; Empyrean's databases are SQLite files, and\n\
               the world database is world_pack_path.)",
        keys: &[
            e(
                "shard_db_path",
                "ShardDbPath",
                "The shard (characters) database, a SQLite file created on first start.",
            ),
            e(
                "auth_db_path",
                "AuthDbPath",
                "The authentication (accounts) database, a SQLite file created on first\n\
                 start; it may be the same file as shard_db_path.",
            ),
        ],
    },
    Section {
        toml: &["offline"],
        ace: &["Offline"],
        array: false,
        help: "Maintenance run before the world starts. The shard should not be in use by a running world.",
        keys: &[
            k(
                "purge_deleted_characters",
                "PurgeDeletedCharacters",
                "Permanently delete characters (and their objects) deleted longer than\n\
                 purge_deleted_characters_days ago.",
            ),
            k(
                "purge_deleted_characters_days",
                "PurgeDeletedCharactersDays",
                "Days a character must have been deleted before it can be purged.",
            ),
            k(
                "purge_orphaned_biotas",
                "PurgeOrphanedBiotas",
                "Purge objects disconnected from the world. This can take a while: run it once every\n\
                 few months, not at every start.",
            ),
            k(
                "prune_deleted_characters_from_friend_lists",
                "PruneDeletedCharactersFromFriendLists",
                "Remove deleted characters from every friend list.",
            ),
            k(
                "prune_deleted_objects_from_shortcut_bars",
                "PruneDeletedObjectsFromShortcutBars",
                "Remove shortcuts to objects that no longer exist.",
            ),
            k(
                "prune_deleted_characters_from_squelch_lists",
                "PruneDeletedCharactersFromSquelchLists",
                "Remove deleted characters from every squelch list (account squelches are kept).",
            ),
        ],
    },
    Section {
        toml: &["ddd"],
        ace: &["DDD"],
        array: false,
        help: "Patching the client's dat files from the server's.",
        keys: &[
            k("enable_dat_patching", "EnableDATPatching", "Let the server patch client dat files from its own."),
            k(
                "precache_compressed_dat_files",
                "PrecacheCompressedDATFiles",
                "At startup, precache every dat file that would be sent compressed.",
            ),
        ],
    },
    Section {
        toml: &["dat_overlay"],
        ace: &["DatOverlay"],
        array: false,
        help: "The world's data overlay: the records the world adds, replaces and deletes over the base\n\
               data files (`dat_files_directory`), as `empyrean-import dat-overlay` writes them. The\n\
               server reads its world as the base with the overlay over it, and patches clients that\n\
               keep overlays of their own to the same world.",
        keys: &[
            e(
                "path",
                "Path",
                "The overlay folder. Empty: the world is the base files.",
            ),
            Key {
                unset: "true",
                ..e(
                    "patching",
                    "Patching",
                    "Patch a client that keeps overlays to the world (the manifest, the revisions and the\n\
                     world's cell records). Off, such a client missing the overlay is refused.",
                )
            },
            Key {
                unset: "60000",
                ..e(
                    "records_per_minute",
                    "RecordsPerMinute",
                    "How many patch records a minute a client that keeps overlays is sent, at most 32 a\n\
                     world tick. 0 keeps ACE's 1,000 a minute shared by every session; a retail client\n\
                     always has ACE's.",
                )
            },
            Key {
                unset: "1048576",
                ..e(
                    "bytes_per_second",
                    "BytesPerSecond",
                    "The most patch bytes a second such a client is sent, so its 128 KiB receive buffer\n\
                     does not overflow between the frames it reads it in.",
                )
            },
        ],
    },
    Section {
        toml: &["era"],
        ace: &["Era"],
        array: false,
        help: "The era the world plays. Every era speaks the end-of-retail client protocol; the era\n\
               selects the world's rules (start positions, level cap, what the character list tells\n\
               the client). The keys after `profile` turn the era's systems on or off for this world;\n\
               a key left out keeps the profile's value. The server refuses what the world lacks, and\n\
               its status document announces the whole set to the client.",
        keys: &[
            e(
                "profile",
                "Profile",
                "\"eor\" (the end of retail, ACE's rules) or \"infiltration\" (February 2005). world.pack\n\
                 must have been built for the same era (`empyrean-import --era <era>`); the server refuses\n\
                 to start otherwise.",
            ),
            // The systems, in `EraFeatures` order. Each defaults to the profile's value: the end
            // of retail has every one but spell research and the oath's experience cost; February
            // 2005 has trade, housing, apartments, tinkering, cantrips, chess and the oath's cost.
            system("ratings", "Ratings", "Damage, critical, healing and the other ratings."),
            system(
                "consolidated_weapon_skills",
                "ConsolidatedWeaponSkills",
                "The 2012 weapon skills (Heavy, Light, Finesse, Missile Weapons) in place of the old ones.",
            ),
            system(
                "item_spell_auras",
                "ItemSpellAuras",
                "Weapon item spells that are auras on the wielder.",
            ),
            system(
                "assessed_armor_and_ratings",
                "AssessedArmorAndRatings",
                "Assessing a player shows its armour levels and ratings.",
            ),
            system(
                "swear_to_lower_level",
                "SwearToLowerLevel",
                "Swearing allegiance to a patron of lower level.",
            ),
            system(
                "pre_order_items_and_rares",
                "PreOrderItemsAndRares",
                "The pre-order gifts at login, and rares dropped by creatures.",
            ),
            system("dual_wield", "DualWield", "A weapon in the off hand."),
            system(
                "weapon_masteries",
                "WeaponMasteries",
                "The heritage weapon masteries.",
            ),
            system(
                "innate_augmentations",
                "InnateAugmentations",
                "The augmentation each heritage is born with.",
            ),
            system(
                "aetheria",
                "Aetheria",
                "Aetheria: wielding it in the sigil slots, and making it from coalesced aetheria.",
            ),
            system(
                "luminance",
                "Luminance",
                "Luminance: earning it, and spending it on luminance augmentations.",
            ),
            system("contracts", "Contracts", "The contract tracker: taking and abandoning contracts."),
            system("titles", "Titles", "Character titles: being granted one, and choosing the one shown."),
            system("cloaks", "Cloaks", "Wearing a cloak in the cloak slot."),
            system("trinkets", "Trinkets", "Wearing a trinket in the trinket slot."),
            system(
                "journal",
                "Journal",
                "The client's journal (a file on the player's machine; the server only announces it).",
            ),
            system("trade", "Trade", "Secure trade between players."),
            system(
                "housing",
                "Housing",
                "Cottages, villas and mansions: buying, renting and entering them. Off turns\n\
                 apartments off too unless apartments is set.",
            ),
            system(
                "apartments",
                "Apartments",
                "Apartments, house recalls and allegiance storage. On turns housing on.",
            ),
            system(
                "tinkering",
                "Tinkering",
                "Salvaging into salvage bags, and the tinkering and salvage recipes.",
            ),
            system("cantrips", "Cantrips", "Cantrips on generated loot."),
            system_off(
                "spell_research",
                "SpellResearch",
                "Learning spells by researching their formulas (gone by 2002; off in every profile).\n\
                 On, a client's formula test casts the spell it makes and teaches it if it is new.",
            ),
            system("chess", "Chess", "Chess on the game boards."),
            system_off(
                "swear_xp_cost",
                "SwearXpCost",
                "An oath of allegiance costs unassigned experience once a character has broken from\n\
                 a patron (before Throne of Destiny; on in infiltration).",
            ),
        ],
    },
];

/// An ACE setting (a key or a whole section) that Empyrean's configuration no longer has.
#[derive(Debug, Clone, Copy)]
pub struct Removed {
    /// Its dotted TOML path as earlier `empyrean.toml` files wrote it.
    pub toml: &'static str,
    /// Its dotted ACE (`Config.js`) path.
    pub ace: &'static str,
    /// Why it is gone.
    pub why: &'static str,
}

const MYSQL_WHY: &str =
    "ACE's MySQL connection; Empyrean's databases are files ([database], world_pack_path)";
const UPDATES_WHY: &str = "ACE's world-database update steps are not ported; world content changes by rebuilding world.pack with empyrean-import";

/// The settings removed from Empyrean's configuration. A file that
/// still has one loads; each is reported once.
pub const REMOVED: &[Removed] = &[
    Removed {
        toml: "server.threading",
        ace: "Server.Threading",
        why: "the world runs on one thread; ACE's thread counts and multi-threaded landblock ticking are not ported",
    },
    Removed { toml: "server.mods_directory", ace: "Server.ModsDirectory", why: "mods are not ported" },
    Removed {
        toml: "server.world_database_precaching",
        ace: "Server.WorldDatabasePrecaching",
        why: "world content is read from the memory-mapped world.pack as it is needed",
    },
    Removed { toml: "mysql.authentication", ace: "MySql.Authentication", why: MYSQL_WHY },
    Removed { toml: "mysql.shard", ace: "MySql.Shard", why: MYSQL_WHY },
    Removed { toml: "mysql.world", ace: "MySql.World", why: MYSQL_WHY },
    Removed { toml: "offline.auto_update_world_database", ace: "Offline.AutoUpdateWorldDatabase", why: UPDATES_WHY },
    Removed {
        toml: "offline.auto_server_update_check",
        ace: "Offline.AutoServerUpdateCheck",
        why: "ACE's check for a newer ACE release does not apply to Empyrean",
    },
    Removed {
        toml: "offline.auto_apply_world_customizations",
        ace: "Offline.AutoApplyWorldCustomizations",
        why: UPDATES_WHY,
    },
    Removed {
        toml: "offline.world_customization_added_paths",
        ace: "Offline.WorldCustomizationAddedPaths",
        why: UPDATES_WHY,
    },
    Removed {
        toml: "offline.recurse_world_customization_paths",
        ace: "Offline.RecurseWorldCustomizationPaths",
        why: UPDATES_WHY,
    },
    Removed {
        toml: "offline.auto_apply_database_updates",
        ace: "Offline.AutoApplyDatabaseUpdates",
        why: "ACE's SQL update scripts are not ported; the store creates and migrates its own schema",
    },
];

/// A key Empyrean renamed. The old name keeps working, with a warning, in the release that renamed
/// it; the next release drops the entry, and the old name is then an unknown key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Renamed {
    /// Its dotted TOML path as earlier files wrote it.
    pub old: &'static str,
    /// Its dotted TOML path now (a key of [`SECTIONS`], outside an array of tables).
    pub new: &'static str,
    /// The release that renamed it (`empyrean-common`'s version then); the entry is removed once
    /// the version moves past it.
    pub since: &'static str,
}

/// The keys renamed in this release, read under their old names with a warning.
pub const RENAMED: &[Renamed] = &[];

/// A renamed key a file used under its old name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deprecated {
    /// The rename.
    pub renamed: Renamed,
    /// Whether the old key's value was read: `false` when the file also has the new key, which
    /// wins.
    pub read: bool,
}

/// The section earlier files called `[mysql]`, now `[database]`; it is not read.
pub const RENAMED_MYSQL: &str = "mysql";

const RENAMED_WHY: &str = "[mysql] is now [database]: move the key there (this one is not read)";

fn removed_by_toml(dotted: &str) -> Option<&'static Removed> {
    REMOVED.iter().find(|r| r.toml == dotted)
}

fn removed_by_ace(dotted: &str) -> Option<&'static Removed> {
    REMOVED.iter().find(|r| r.ace == dotted)
}

fn section(path: &[&str]) -> Option<&'static Section> {
    SECTIONS.iter().find(|s| s.toml == path)
}

fn section_by_ace(path: &[&str]) -> Option<&'static Section> {
    SECTIONS.iter().find(|s| s.ace == path)
}

/// Joins a path with the key for messages (`server.network.port`).
fn dotted(path: &[&str], key: &str) -> String {
    let mut s = path.join(".");
    if !s.is_empty() {
        s.push('.');
    }
    s.push_str(key);
    s
}

/// A key a file has that is not read, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ignored {
    /// The dotted path (TOML for `empyrean.toml`, ACE's for `Config.js`).
    pub key: String,
    /// Why it is not read.
    pub why: String,
}

/// What a TOML table holds that is not read, and the renamed keys it holds under their old names.
#[derive(Debug, Default)]
struct Report<'a> {
    unknown: Vec<String>,
    ignored: Vec<Ignored>,
    renames: &'a [Renamed],
    moved: Vec<(Renamed, Value)>,
}

impl Report<'_> {
    fn ignore(&mut self, key: String, why: &str) {
        if !self.ignored.iter().any(|i| i.key == key) {
            self.ignored.push(Ignored {
                key,
                why: why.to_owned(),
            });
        }
    }
}

/// Renames one table's keys to the serde names, in file order. `path` is the table's TOML path.
fn to_ace_table(table: Table, path: &[&str], report: &mut Report<'_>) -> Table {
    let this = section(path);
    let mut out = Table::new();
    for (key, value) in table {
        if let Some(key_def) = this.and_then(|s| s.keys.iter().find(|k| k.toml == key)) {
            out.insert(key_def.ace.to_owned(), value);
            continue;
        }
        let here = dotted(path, &key);
        if let Some(renamed) = report.renames.iter().find(|r| r.old == here) {
            report.moved.push((*renamed, value));
            continue;
        }
        if let Some(removed) = removed_by_toml(&here) {
            report.ignore(here, removed.why);
            continue;
        }
        if path.is_empty() && key == RENAMED_MYSQL {
            match value {
                Value::Table(t) if !t.is_empty() => {
                    for inner in t.keys() {
                        let d = dotted(&[RENAMED_MYSQL], inner);
                        let why = removed_by_toml(&d).map_or(RENAMED_WHY, |r| r.why);
                        report.ignore(d, why);
                    }
                }
                _ => report.ignore(here, RENAMED_WHY),
            }
            continue;
        }
        let mut child_path: Vec<&str> = path.to_vec();
        child_path.push(&key);
        let Some(child) = section(&child_path) else {
            report.unknown.push(here);
            continue;
        };
        let ace_name = child.ace[child.ace.len() - 1].to_owned();
        let value = match value {
            Value::Table(t) if !child.array => Value::Table(to_ace_table(t, &child_path, report)),
            Value::Array(items) if child.array => Value::Array(
                items
                    .into_iter()
                    .map(|item| match item {
                        Value::Table(t) => Value::Table(to_ace_table(t, &child_path, report)),
                        other => other,
                    })
                    .collect(),
            ),
            // The wrong shape: passed on, for serde to reject.
            other => other,
        };
        out.insert(ace_name, value);
    }
    out
}

/// A parsed `empyrean.toml`, and the keys it had that are not read.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// The configuration.
    pub config: MasterConfiguration,
    /// Unknown keys and sections, as dotted TOML paths, in file order.
    pub unknown_keys: Vec<String>,
    /// Removed ACE settings and keys under the renamed `[mysql]`, in file order, with the reason.
    pub ignored: Vec<Ignored>,
    /// Renamed keys the file has under their old names, in file order.
    pub deprecated: Vec<Deprecated>,
}

/// The ACE (serde) path of the TOML key at dotted `path`: its sections' names and its own. `None`
/// when no section outside an array of tables has that key.
fn ace_path_of(path: &str) -> Option<Vec<&'static str>> {
    let parts: Vec<&str> = path.split('.').collect();
    let (key, sections) = parts.split_last()?;
    let this = section(sections)?;
    if (1..=sections.len()).any(|n| section(&sections[..n]).is_none_or(|s| s.array)) {
        return None;
    }
    let key = this.keys.iter().find(|k| k.toml == *key)?;
    let mut out = this.ace.to_vec();
    out.push(key.ace);
    Some(out)
}

/// Puts `value` at the ACE path `path` of `table` unless a value is already there (the new key
/// wins). Whether it was put.
fn put_unless_present(table: &mut Table, path: &[&str], value: Value) -> bool {
    let Some((key, sections)) = path.split_last() else {
        return false;
    };
    let mut at = table;
    for s in sections {
        let entry = at
            .entry((*s).to_owned())
            .or_insert_with(|| Value::Table(Table::new()));
        let Value::Table(inner) = entry else {
            return false;
        };
        at = inner;
    }
    if at.contains_key(*key) {
        return false;
    }
    at.insert((*key).to_owned(), value);
    true
}

/// Parses `empyrean.toml`.
///
/// # Errors
/// When the text is not TOML, or a value does not fit its setting.
pub fn from_toml_str(text: &str) -> Result<Parsed, ConfigError> {
    from_toml_str_with_renames(text, RENAMED)
}

/// [`from_toml_str`] with `renames` in place of [`RENAMED`]: each old key is read as its new one
/// (unless the file also has the new key) and reported in [`Parsed::deprecated`]. A rename whose
/// new key is not a setting leaves the old key unknown.
///
/// # Errors
/// As [`from_toml_str`].
pub fn from_toml_str_with_renames(text: &str, renames: &[Renamed]) -> Result<Parsed, ConfigError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let table: Table = toml::from_str(text).map_err(ConfigError::Toml)?;
    let mut report = Report {
        renames,
        ..Report::default()
    };
    let mut ace = to_ace_table(table, &[], &mut report);
    let mut deprecated = Vec::new();
    for (renamed, value) in std::mem::take(&mut report.moved) {
        match ace_path_of(renamed.new) {
            Some(path) => deprecated.push(Deprecated {
                renamed,
                read: put_unless_present(&mut ace, &path, value),
            }),
            None => report.unknown.push(renamed.old.to_owned()),
        }
    }
    let config: MasterConfiguration = Value::Table(ace).try_into().map_err(ConfigError::Toml)?;
    Ok(Parsed {
        config,
        unknown_keys: report.unknown,
        ignored: report.ignored,
        deprecated,
    })
}

/// The warnings a loaded `empyrean.toml` at `file` gives at start-up, one line each: every unknown
/// key, every removed setting with the reason, and every renamed key used under its old name.
#[must_use]
pub fn warnings(parsed: &Parsed, file: &std::path::Path) -> Vec<String> {
    let file = file.display();
    let mut out: Vec<String> = parsed
        .unknown_keys
        .iter()
        .map(|key| format!("Configuration: unknown key `{key}` in {file} is ignored"))
        .collect();
    out.extend(parsed.ignored.iter().map(|i| {
        format!(
            "Configuration: `{}` in {file} is not read: {}",
            i.key, i.why
        )
    }));
    out.extend(parsed.deprecated.iter().map(|d| {
        let r = d.renamed;
        if d.read {
            format!(
                "Configuration: `{}` in {file} is now `{}` (renamed in {}); the old name is read in this release only, so rename it before the next",
                r.old, r.new, r.since
            )
        } else {
            format!(
                "Configuration: `{}` in {file} is not read: it is now `{}` (renamed in {}), which the file also sets; remove the old key",
                r.old, r.new, r.since
            )
        }
    }));
    out
}

/// A `Config.js` converted: the configuration it holds and the keys that were dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    /// The configuration (what the `empyrean.toml` written from it loads as).
    pub config: MasterConfiguration,
    /// Every `Config.js` key the configuration has no place for, as dotted ACE paths sorted by key,
    /// with the reason.
    pub dropped: Vec<Ignored>,
}

const UNKNOWN_WHY: &str = "not a setting ACE or Empyrean has";

/// Lists the keys of a `Config.js` object at `path` (ACE names) that the schema has no place for.
fn walk_config_js(value: &Json, path: &[&str], dropped: &mut Vec<Ignored>) {
    let Some(map) = value.as_object() else { return };
    let this = section_by_ace(path);
    for (key, inner) in map {
        if this.is_some_and(|s| s.keys.iter().any(|k| k.ace == key)) {
            continue;
        }
        let here = dotted(path, key);
        let mut child_path: Vec<&str> = path.to_vec();
        child_path.push(key);
        if let Some(child) = section_by_ace(&child_path) {
            match inner {
                Json::Array(items) if child.array => {
                    for item in items {
                        walk_config_js(item, &child_path, dropped);
                    }
                }
                other => walk_config_js(other, &child_path, dropped),
            }
            continue;
        }
        let why = removed_by_ace(&here).map_or(UNKNOWN_WHY, |r| r.why);
        if !dropped.iter().any(|d| d.key == here) {
            dropped.push(Ignored {
                key: here,
                why: why.to_owned(),
            });
        }
    }
}

/// Reads ACE's `Config.js` for the one-time conversion to `empyrean.toml`.
///
/// # Errors
/// When the text is not JSON, or a value does not fit its setting.
pub fn from_config_js_str(text: &str) -> Result<Converted, ConfigError> {
    let value = ConfigManager::parse_json(text)?;
    let mut dropped = Vec::new();
    walk_config_js(&value, &[], &mut dropped);
    let config = serde_json::from_value(value).map_err(ConfigError::Json)?;
    Ok(Converted { config, dropped })
}

/// A JSON value (the serde form of a setting) as a TOML literal; `None` for `null`.
fn toml_literal(value: &Json) -> Option<String> {
    fn convert(value: &Json) -> Option<Value> {
        Some(match value {
            Json::Null | Json::Object(_) => return None,
            Json::Bool(b) => Value::Boolean(*b),
            Json::Number(n) => match (n.as_i64(), n.as_f64()) {
                (Some(i), _) => Value::Integer(i),
                (None, Some(f)) => Value::Float(f),
                (None, None) => return None,
            },
            Json::String(s) => Value::String(s.clone()),
            Json::Array(items) => Value::Array(items.iter().filter_map(convert).collect()),
        })
    }
    convert(value).map(|v| v.to_string())
}

fn lookup<'a>(root: &'a Json, path: &[&str]) -> Option<&'a Json> {
    path.iter().try_fold(root, |v, key| v.get(key))
}

fn push_comment(out: &mut String, text: &str) {
    for line in text.lines() {
        if line.is_empty() {
            out.push_str("#\n");
        } else {
            out.push_str("# ");
            out.push_str(line);
            out.push('\n');
        }
    }
}

fn default_text(value: Option<&Json>) -> String {
    value
        .and_then(toml_literal)
        .unwrap_or_else(|| "not set".to_owned())
}

/// The key's origin, for its comment.
fn origin(key: &Key, ace_path: &[&str]) -> String {
    if key.in_ace {
        format!("ACE Config.js: {}.{}", ace_path.join("."), key.ace)
    } else {
        "Empyrean only".to_owned()
    }
}

fn push_key(
    out: &mut String,
    key: &Key,
    ace_path: &[&str],
    value: Option<&Json>,
    default: Option<&Json>,
) {
    out.push('\n');
    push_comment(
        out,
        &format!(
            "default: {} ({})",
            if key.unset == "\"\"" || default.is_some_and(|d| !d.is_null()) {
                default_text(default)
            } else {
                "the profile's".to_owned()
            },
            origin(key, ace_path)
        ),
    );
    push_comment(out, key.help);
    match value.and_then(toml_literal) {
        Some(literal) => out.push_str(&format!("{} = {literal}\n", key.toml)),
        None => out.push_str(&format!("# {} = {}\n", key.toml, key.unset)),
    }
}

/// The header of every written `empyrean.toml`.
pub const HEADER: &str = "\
# Empyrean configuration: empyrean.toml.
#
# This file is the server's only configuration. Every key is optional: a key left out keeps its
# default, which each key's comment gives with the ACE Config.js setting it stands for (or
# \"Empyrean only\"). The server reads empyrean.toml from the working directory, else from beside
# the server executable; `--config <path>` names another file. With neither, it runs on the
# defaults.
#
# Paths: a leading ~ is your home folder (~/ac, or ~\\ac on Windows); an absolute path is used as
# it is; a relative path is relative to the folder this file is in (the working directory when the
# server runs on the defaults, with no file). The server logs where each path resolved at info level.
#
# The server reads no environment variables: this file is all of its configuration.
# `--config <path>` names the file, and `--status <ip>:<port>` on the command line overrides
# server.status_address.
#
# ACE's Config.js is not read. Convert one once with
#     empyrean-server --write-config --from Config.js --out empyrean.toml
# which drops the settings Empyrean does not have and lists them. `empyrean-server --write-config
# [<path>]` writes the configuration in use as a file like this one.
";

/// Writes `config` as a fully commented `empyrean.toml` (`--write-config`).
#[must_use]
pub fn to_toml_string(config: &MasterConfiguration) -> String {
    to_toml_string_with_note(config, None)
}

/// [`to_toml_string`] with `note` (comment lines, such as what a conversion dropped) after the
/// header.
///
/// # Panics
/// Never in practice: the configuration types always serialise.
#[must_use]
pub fn to_toml_string_with_note(config: &MasterConfiguration, note: Option<&str>) -> String {
    let value = serde_json::to_value(config).expect("MasterConfiguration serialises");
    let defaults = serde_json::to_value(MasterConfiguration::default())
        .expect("MasterConfiguration serialises");
    let mut out = String::from(HEADER);
    if let Some(note) = note {
        out.push_str("#\n");
        push_comment(&mut out, note);
    }

    for s in SECTIONS {
        let items = lookup(&value, s.ace);
        let default_items = lookup(&defaults, s.ace);
        if s.array {
            let entries = items
                .and_then(Json::as_array)
                .map_or(&[][..], Vec::as_slice);
            let default_ids: Vec<String> = default_items
                .and_then(Json::as_array)
                .map_or(&[][..], Vec::as_slice)
                .iter()
                .filter_map(|e| e.get("Id").and_then(Json::as_str).map(str::to_owned))
                .collect();
            out.push_str(&format!("\n# {}\n", "-".repeat(94)));
            push_comment(
                &mut out,
                &format!(
                    "[[{}]], default: {} entries ({}) (ACE Config.js: {}).",
                    s.toml.join("."),
                    default_ids.len(),
                    default_ids.join(", "),
                    s.ace.join("."),
                ),
            );
            push_comment(&mut out, s.help);
            for key in s.keys {
                push_comment(&mut out, &format!("  {}: {}", key.toml, key.help));
            }
            for entry in entries {
                out.push_str(&format!("\n[[{}]]\n", s.toml.join(".")));
                for key in s.keys {
                    if let Some(literal) = entry.get(key.ace).and_then(toml_literal) {
                        out.push_str(&format!("{} = {literal}\n", key.toml));
                    }
                }
            }
            continue;
        }

        out.push_str(&format!("\n# {}\n[{}]\n", "-".repeat(94), s.toml.join(".")));
        push_comment(&mut out, s.help);
        for key in s.keys {
            push_key(
                &mut out,
                key,
                s.ace,
                items.and_then(|v| v.get(key.ace)),
                default_items.and_then(|v| v.get(key.ace)),
            );
        }
        // An empty array-of-tables child cannot be written as [[...]]; state it in this table.
        for child in SECTIONS
            .iter()
            .filter(|c| c.array && c.toml.len() == s.toml.len() + 1 && c.toml.starts_with(s.toml))
        {
            let empty = lookup(&value, child.ace)
                .and_then(Json::as_array)
                .is_some_and(Vec::is_empty);
            if empty {
                out.push_str(&format!(
                    "\n# none.\n{} = []\n",
                    child.toml[child.toml.len() - 1]
                ));
            }
        }
    }
    out
}

/// The comment a converted file carries: where it came from and every key dropped.
#[must_use]
pub fn conversion_note(source: &str, dropped: &[Ignored]) -> String {
    let mut note = format!("Converted from {source}.");
    if dropped.is_empty() {
        note.push_str(" Every key it had is kept.");
    } else {
        note.push_str(" Dropped (Empyrean has no such setting):");
        for d in dropped {
            note.push_str(&format!("\n  {}: {}", d.key, d.why));
        }
    }
    note.push_str(
        "\nIts paths are copied as written. A relative path in Config.js was relative to ACE's working\n\
         directory; here it is relative to the folder this file is in: check the paths below.",
    );
    note
}

/// Writes `text` at `path`: the one file writer behind `--write-config` and the `config-write`
/// console command. Without `overwrite`, an existing file is left alone and the error is
/// [`std::io::ErrorKind::AlreadyExists`].
///
/// # Errors
/// When the file exists (and `overwrite` is false) or cannot be written.
pub fn write_text_file(text: &str, path: &std::path::Path, overwrite: bool) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut file = if overwrite {
        std::fs::File::create(path)?
    } else {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?
    };
    file.write_all(text.as_bytes())
}

/// Writes `config` as a commented `empyrean.toml` at `path` ([`write_text_file`]).
///
/// # Errors
/// When the file exists (and `overwrite` is false) or cannot be written.
pub fn write_toml_file(
    config: &MasterConfiguration,
    path: &std::path::Path,
    overwrite: bool,
) -> std::io::Result<()> {
    write_text_file(&to_toml_string(config), path, overwrite)
}
