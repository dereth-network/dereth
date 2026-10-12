//! `dereth-network-hub`: run the hub, or manage its registry.

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dereth_network_hub::community;
use dereth_network_hub::registry::{ImportOptions, Registration, Registry};
use dereth_network_hub::server::{self, HubConfig};
use dereth_network_protocol::Federation;

const USAGE: &str = "\
dereth-network-hub -- the Dereth Network's hub and world registry

  dereth-network-hub serve [--listen <addr>] [--insecure-listen] [--run-for <secs>]
                           [--trusted-proxy <ip or cidr>]... [--trusted-proxies-file <file>]...
                           [--client-address-header <name>] [--proxy-hops <n>]
                           [--deny-fetch <ip or cidr>]... [--deny-fetch-file <file>]...
                           [--public-url <url>] [--site-origin <origin>]... [--admin <discord id>]...
                           [--discord-client-id <id>] [--discord-client-secret-file <path>]
                           [--approvals-per-day <n>] [--min-account-age-days <n>]
                           [--pending-days <n>]
                           [--wire-log <file>]    one line per link frame: its type, never its body
  dereth-network-hub --version
  dereth-network-hub --registry-schema            the registry schema this build reads
  dereth-network-hub self-update --check|--apply [--policy patch|minor] [--source <url>]
                                                  install a newer release over this binary
  dereth-network-hub register <server.json>       print the new world and its key (once)
  dereth-network-hub update <server.json>         replace a managed world's registration
  dereth-network-hub list                         every world, pending ones included
  dereth-network-hub federation <slug> on|off|unset
  dereth-network-hub claim <slug> \"<the ownership evidence you reviewed>\"
  dereth-network-hub verify <slug> \"<the evidence you reviewed>\"
                                                  vouch that a registered world runs at its
                                                  endpoint: verified as a proof verifies it
  dereth-network-hub rotate-key <slug>            a new key; the old one stops working
  dereth-network-hub revoke-key <slug>
  dereth-network-hub import-community <Servers.xml> | --fetch [--url <url>]
                     [--apply] [--skip-conflicts] [--retire-after <n>] [--summary]
                                                  a dry run unless --apply
  dereth-network-hub export-community             the registry as a community list
  dereth-network-hub audit <slug>                 a world's last 100 registry events
  dereth-network-hub backup <file>                a consistent copy of the registry, while it runs
  dereth-network-hub reports [--all]              the open reports (every one with --all)
  dereth-network-hub report <id> dismiss|revoke \"<note>\"   revoke: federation off, key revoked, locked
  dereth-network-hub unlock <slug>                lift a revocation's lock
  dereth-network-hub remove <slug>                remove a world for good, freeing its slug
  dereth-network-hub unlist|relist <slug>         take a world out of the public lists, or back
  dereth-network-hub assign <slug> <discord id>   hand a managed world to an operator
  dereth-network-hub operators                    every operator
  dereth-network-hub end-sessions                 end every sign-in (after a restore); all sign in again
  dereth-network-hub disable-operator <discord id>, enable-operator <discord id>

  --db <path>       the registry (default: hub.sqlite, or DERETH_HUB_DB)
  --domain <name>   the registry's web domain (default: dereth.network)
  --public-url <url>, --site-origin <origin>   the hub's own address and its site's
                    (default: https://hub.<domain> and https://<domain>)

A world's game endpoint is never the domain itself, hub.<domain>, api.<domain>, or the host of
--public-url or a --site-origin. Any other name under the domain is the hub operator's: allowed
from the command line and for an --admin, refused for every other operator and in an import. A
world's links point at --public-url and the first --site-origin, so give these to every command
(or set DERETH_HUB_PUBLIC_URL and DERETH_HUB_SITE_ORIGINS) when they are not the defaults.

`serve` listens on 127.0.0.1:8650 unless --listen names another address. The hub speaks plain
HTTP and ws://, so an address other than loopback is refused unless a TLS proxy in front of it is
named with --trusted-proxy, or --insecure-listen says the keys may travel in the clear. A public
hub needs a proxy that also holds each request until its head has arrived. A key is printed once
and only its digest is kept: store it on the world's server.

Behind proxies, name each (Cloudflare's ranges too) with --trusted-proxy, or list them one to a
line in a --trusted-proxies-file (# starts a comment); --proxy-hops is how many proxies a request
crosses (1, the default, for the hub's own reverse proxy; 2 for Cloudflare and then it), and the
client is read that many entries from the right of X-Forwarded-For; --client-address-header
is x-forwarded-for (the default) or cf-connecting-ip. --deny-fetch (and --deny-fetch-file, one
range to a line) names ranges a verification page is never read from, whatever else allows them:
list the hub's own public address, its IPv6 prefix and its local network. The operator API (world operators signing in
with Discord) is on when --public-url (the hub's own address, DERETH_HUB_PUBLIC_URL), a
--site-origin (DERETH_HUB_SITE_ORIGINS, comma-separated) and --discord-client-id
(DERETH_HUB_DISCORD_CLIENT_ID) are given; the client secret is read from the environment variable
DERETH_HUB_DISCORD_CLIENT_SECRET, or the file --discord-client-secret-file names, never from the
command line. --admin (DERETH_HUB_ADMINS) names the Discord ids that may use the admin routes
and register game endpoints under the domain.
--approvals-per-day (500) is the hub's ceiling on worlds joining the network a day, above each
operator's own 3; --min-account-age-days (30) is how old a Discord account must be to register or
claim a world; --pending-days (14) is how long a registered world may wait to be verified before it
is removed and its slug freed.

`import-community` reads the community list from a file, or with --fetch from the community
repository (or --url), at most 2 MiB within 30 seconds; a list that does not arrive or does not
read changes nothing. --skip-conflicts applies everything but the conflicts and refused records,
and names them. An applied import counts, for each unclaimed listing the list no longer holds,
the imports in a row it has been missing from, and after --retire-after of them (3) takes it out
of the public lists; it is listed again when it returns. --summary prints one line of counts in
place of the plan.
`self-update` lists the hub's releases (the GitHub repository this build came from, or --source:
https://github.com/<owner>/<name>, or a mirror of its releases API) and takes the newest its
--policy allows: patch (this build's major and minor version, and only a release whose binary
reads the same registry schema) or minor (this major version; the default). A major release, a
pre-release and one that failed here before are never installed. --check says what would be
installed and why the others would not be; --apply downloads this platform's archive, refuses it
unless its SHA-256 is the one the release's SHA256SUMS lists (the only check: releases are not
signed), runs the new binary, which must name the release's version and read this registry, backs
the registry up beside it, and moves this binary aside as <name>.previous-v<version> and the new one
into its place. It needs write access to the binary's folder, and starts or stops nothing: restart
the hub to run the new binary. It exits 0 when a release was (or, with --check, would be)
installed, 3 when none is to be, and 1 when it refused or failed.
For development only: --allow-loopback (loopback endpoints, pages and community lists), --discord-api,
--discord-authorize (a stand-in for Discord).";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("dereth-network-hub: {e}");
            ExitCode::FAILURE
        }
    }
}

/// What `--version` prints: the program and its version.
fn version_line() -> String {
    format!("dereth-network-hub {}", env!("CARGO_PKG_VERSION"))
}

fn take(args: &mut Vec<String>, flag: &str) -> Result<Option<String>, String> {
    match args.iter().position(|a| a == flag) {
        None => Ok(None),
        Some(i) => {
            if i + 1 >= args.len() {
                return Err(format!("{flag} needs a value"));
            }
            let v = args.remove(i + 1);
            args.remove(i);
            Ok(Some(v))
        }
    }
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    match args.iter().position(|a| a == flag) {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    }
}

/// Whether the hub may listen on `listen`: loopback always; any other address only behind a TLS
/// proxy (`proxied`) or when the operator says the keys may travel in the clear (`insecure`).
fn check_listen(listen: std::net::SocketAddr, proxied: bool, insecure: bool) -> Result<(), String> {
    if listen.ip().is_loopback() || proxied {
        return Ok(());
    }
    if insecure {
        server::log(
            "warn",
            &format!(
                "listening on {listen}, which is not loopback, with no TLS proxy named: world keys travel in the clear"
            ),
        );
        return Ok(());
    }
    Err(format!(
        "--listen {listen} is not loopback: name the TLS proxy in front of the hub with --trusted-proxy, or pass --insecure-listen"
    ))
}

/// A value from the command line, or else the environment.
fn flag_or_env(args: &mut Vec<String>, flag: &str, var: &str) -> Result<Option<String>, String> {
    Ok(take(args, flag)?.or_else(|| std::env::var(var).ok().filter(|v| !v.trim().is_empty())))
}

/// An origin: `http(s)://host[:port]`, nothing after it.
fn check_origin(o: &str) -> Result<String, String> {
    let o = o.trim().trim_end_matches('/');
    let ok = (o.starts_with("https://") || o.starts_with("http://"))
        && o.split_once("://")
            .is_some_and(|(_, h)| !h.is_empty() && !h.contains(['/', '?', '#', '@']));
    if ok {
        Ok(o.to_ascii_lowercase())
    } else {
        Err(format!("{o}: an origin, https://host[:port]"))
    }
}

/// The hub's public URL and its site's origins, from the command line or else the environment:
/// the serving hub's own, and the names every command keeps worlds' endpoints off and links to.
fn web_names(args: &mut Vec<String>) -> Result<(Option<String>, Vec<String>), String> {
    let mut public_url = None;
    if let Some(u) = flag_or_env(args, "--public-url", "DERETH_HUB_PUBLIC_URL")? {
        dereth_network_hub::fetch::Url::parse(&u).map_err(|e| format!("--public-url {u}: {e}"))?;
        public_url = Some(u.trim_end_matches('/').to_owned());
    }
    let mut origins = Vec::new();
    while let Some(o) = take(args, "--site-origin")? {
        origins.push(check_origin(&o)?);
    }
    if origins.is_empty() {
        if let Ok(list) = std::env::var("DERETH_HUB_SITE_ORIGINS") {
            for o in list.split(',').filter(|o| !o.trim().is_empty()) {
                origins.push(check_origin(o)?);
            }
        }
    }
    Ok((public_url, origins))
}

/// The operator API's settings: on when the hub's public URL, a site origin and a Discord client
/// id are all given (the secret comes from the environment or a file).
fn operator_api(args: &mut Vec<String>, config: &mut HubConfig) -> Result<(), String> {
    while let Some(a) = take(args, "--admin")? {
        config.admins.push(a);
    }
    if config.admins.is_empty() {
        if let Ok(list) = std::env::var("DERETH_HUB_ADMINS") {
            config.admins = list
                .split(',')
                .map(|a| a.trim().to_owned())
                .filter(|a| !a.is_empty())
                .collect();
        }
    }
    if let Some(n) = take(args, "--approvals-per-day")? {
        config.approvals_per_day = n
            .parse()
            .map_err(|e| format!("--approvals-per-day {n}: {e}"))?;
    }
    if let Some(n) = take(args, "--min-account-age-days")? {
        config.min_account_age_days = n
            .parse()
            .map_err(|e| format!("--min-account-age-days {n}: {e}"))?;
    }
    if let Some(n) = take(args, "--pending-days")? {
        config.pending_days = n.parse().map_err(|e| format!("--pending-days {n}: {e}"))?;
    }
    config.loopback_allowed = take_flag(args, "--allow-loopback");
    let secret_file = take(args, "--discord-client-secret-file")?;
    let api = take(args, "--discord-api")?;
    let authorize = take(args, "--discord-authorize")?;
    if let Some(id) = flag_or_env(args, "--discord-client-id", "DERETH_HUB_DISCORD_CLIENT_ID")? {
        let mut d = dereth_network_hub::discord::Discord::new(id, secret_file.as_deref())?;
        if let Some(a) = api {
            d.api = a.trim_end_matches('/').to_owned();
        }
        if let Some(a) = authorize {
            d.authorize = a;
        }
        config.discord = Some(d);
    }
    let on =
        config.discord.is_some() && config.public_url.is_some() && !config.site_origins.is_empty();
    if config.discord.is_some() && !on {
        return Err(
            "the operator API needs --public-url and --site-origin as well as the Discord client"
                .into(),
        );
    }
    if on {
        server::log(
            "info",
            &format!(
                "operator API on: sign-in returns to {}; {} admin(s)",
                config.site_origins[0],
                config.admins.len()
            ),
        );
    }
    if config.loopback_allowed {
        server::log(
            "warn",
            "--allow-loopback: loopback endpoints and pages are accepted (development only)",
        );
    }
    Ok(())
}

fn read_json(path: &str) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))
}

fn pretty(v: &serde_json::Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

#[allow(clippy::too_many_lines)]
fn run(mut args: Vec<String>) -> Result<ExitCode, String> {
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    }
    if args == ["--version"] {
        println!("{}", version_line());
        return Ok(ExitCode::SUCCESS);
    }
    if args == ["--registry-schema"] {
        println!("{}", dereth_network_hub::registry::SCHEMA_VERSION);
        return Ok(ExitCode::SUCCESS);
    }
    let db = take(&mut args, "--db")?
        .or_else(|| std::env::var("DERETH_HUB_DB").ok())
        .unwrap_or_else(|| "hub.sqlite".into());
    let domain = take(&mut args, "--domain")?.unwrap_or_else(|| "dereth.network".into());
    let (public_url, site_origins) = web_names(&mut args)?;
    let open = || {
        let mut r = Registry::open(&db, &domain, Box::new(std::time::SystemTime::now))
            .map_err(|e| e.to_string())?;
        r.set_web(public_url.as_deref(), &site_origins);
        Ok::<_, String>(r)
    };
    if args.is_empty() {
        return Err(USAGE.into());
    }
    let command = args.remove(0);
    let arg = |i: usize| {
        args.get(i)
            .cloned()
            .ok_or_else(|| format!("{command}: missing argument\n\n{USAGE}"))
    };
    match command.as_str() {
        "serve" => {
            let mut config = HubConfig {
                db: db.clone(),
                domain: domain.clone(),
                public_url: public_url.clone(),
                site_origins: site_origins.clone(),
                ..HubConfig::default()
            };
            if let Some(l) = take(&mut args, "--listen")? {
                config.listen = l.parse().map_err(|e| format!("--listen {l}: {e}"))?;
            }
            while let Some(p) = take(&mut args, "--trusted-proxy")? {
                config.trusted_proxies.push(p.parse()?);
            }
            while let Some(path) = take(&mut args, "--trusted-proxies-file")? {
                let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
                let ranges = dereth_network_hub::forwarded::read_ranges(&text)
                    .map_err(|e| format!("{path}: {e}"))?;
                config.trusted_proxies.extend(ranges);
            }
            if let Some(h) = take(&mut args, "--client-address-header")? {
                config.client_header = h.parse()?;
            }
            while let Some(p) = take(&mut args, "--deny-fetch")? {
                config.deny_fetch.push(p.parse()?);
            }
            while let Some(path) = take(&mut args, "--deny-fetch-file")? {
                let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
                let ranges = dereth_network_hub::forwarded::read_ranges(&text)
                    .map_err(|e| format!("{path}: {e}"))?;
                config.deny_fetch.extend(ranges);
            }
            if let Some(n) = take(&mut args, "--proxy-hops")? {
                config.proxy_hops = n
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=8).contains(n))
                    .ok_or_else(|| format!("--proxy-hops {n}: 1 to 8"))?;
            }
            operator_api(&mut args, &mut config)?;
            config.wire_log = take(&mut args, "--wire-log")?.map(std::path::PathBuf::from);
            let insecure = take_flag(&mut args, "--insecure-listen");
            let run_for = take(&mut args, "--run-for")?
                .map(|s| s.parse::<f64>().map_err(|e| format!("--run-for {s}: {e}")))
                .transpose()?;
            if !args.is_empty() {
                return Err(format!("serve: unexpected {:?}", args));
            }
            check_listen(config.listen, !config.trusted_proxies.is_empty(), insecure)?;
            let running = server::start(config).map_err(|e| e.to_string())?;
            let stop = Arc::new(AtomicBool::new(false));
            let s = Arc::clone(&stop);
            ctrlc::set_handler(move || s.store(true, Ordering::SeqCst))
                .map_err(|e| e.to_string())?;
            let started = std::time::Instant::now();
            while !stop.load(Ordering::SeqCst)
                && run_for.is_none_or(|r| started.elapsed().as_secs_f64() < r)
            {
                std::thread::sleep(Duration::from_millis(100));
            }
            server::log("info", "stopping");
            running.shutdown();
            Ok(ExitCode::SUCCESS)
        }
        "register" => {
            let r = Registration::from_json(&read_json(&arg(0)?)?).map_err(|e| e.message)?;
            let (server, key) = open()?.register(&r).map_err(|e| e.message)?;
            println!(
                "{}",
                pretty(&serde_json::json!({ "server": server, "key": key }))
            );
            eprintln!("Save the key securely; it cannot be retrieved later.");
            Ok(ExitCode::SUCCESS)
        }
        "update" => {
            let r = Registration::from_json(&read_json(&arg(0)?)?).map_err(|e| e.message)?;
            println!("{}", pretty(&open()?.update(&r).map_err(|e| e.message)?));
            Ok(ExitCode::SUCCESS)
        }
        "list" => {
            println!(
                "{}",
                pretty(&serde_json::Value::Array(
                    open()?.everything().map_err(|e| e.message)?
                ))
            );
            Ok(ExitCode::SUCCESS)
        }
        "reports" => {
            let all = take_flag(&mut args, "--all");
            println!("{}", pretty(&open()?.reports(all).map_err(|e| e.message)?));
            Ok(ExitCode::SUCCESS)
        }
        "report" => {
            let out = open()?
                .resolve_report(
                    &arg(0)?,
                    &arg(1)?,
                    &args.get(2).cloned().unwrap_or_default(),
                    None,
                )
                .map_err(|e| e.message)?;
            println!("{}", pretty(&out));
            Ok(ExitCode::SUCCESS)
        }
        "unlock" => {
            open()?.unlock(&arg(0)?).map_err(|e| e.message)?;
            println!("{}: unlocked", arg(0)?);
            Ok(ExitCode::SUCCESS)
        }
        "remove" => {
            open()?.remove(&arg(0)?).map_err(|e| e.message)?;
            println!("{}: removed", arg(0)?);
            Ok(ExitCode::SUCCESS)
        }
        "unlist" | "relist" => {
            let listed = command == "relist";
            open()?
                .set_listed(&arg(0)?, listed)
                .map_err(|e| e.message)?;
            println!("{}: {command}ed", arg(0)?);
            Ok(ExitCode::SUCCESS)
        }
        "assign" => {
            open()?.assign(&arg(0)?, &arg(1)?).map_err(|e| e.message)?;
            println!(
                "{} now belongs to the operator with Discord id {}",
                arg(0)?,
                arg(1)?
            );
            Ok(ExitCode::SUCCESS)
        }
        "operators" => {
            println!("{}", pretty(&open()?.operators().map_err(|e| e.message)?));
            Ok(ExitCode::SUCCESS)
        }
        "end-sessions" => {
            let n = open()?.end_all_sessions().map_err(|e| e.message)?;
            println!("{n} session(s) ended; every operator and person signs in again");
            Ok(ExitCode::SUCCESS)
        }
        "disable-operator" | "enable-operator" => {
            let disable = command == "disable-operator";
            open()?
                .set_operator_disabled(&arg(0)?, disable)
                .map_err(|e| e.message)?;
            println!(
                "{}: {}",
                arg(0)?,
                if disable {
                    "disabled; its sessions have ended"
                } else {
                    "enabled"
                }
            );
            Ok(ExitCode::SUCCESS)
        }
        "federation" => {
            let flag: Federation = arg(1)?
                .parse()
                .map_err(|()| "federation: on, off or unset".to_owned())?;
            open()?
                .set_federation(&arg(0)?, flag)
                .map_err(|e| e.message)?;
            println!("{}: federation {flag}", arg(0)?);
            Ok(ExitCode::SUCCESS)
        }
        "claim" => {
            let (server, key) = open()?.claim(&arg(0)?, &arg(1)?).map_err(|e| e.message)?;
            println!(
                "{}",
                pretty(&serde_json::json!({ "server": server, "key": key }))
            );
            eprintln!("Save the key securely. The review is recorded in the audit log.");
            Ok(ExitCode::SUCCESS)
        }
        "verify" => {
            let slug = arg(0)?;
            let approval = open()?.vouch(&slug, &arg(1)?).map_err(|e| e.message)?;
            match approval {
                Some(a) => println!(
                    "{slug}: verified by the hub's operator; approval onto the network: {}",
                    a.word()
                ),
                None => println!(
                    "{slug}: verified by the hub's operator; it has no owner, so its federation flag is unchanged"
                ),
            }
            eprintln!("The evidence is recorded in the audit log.");
            Ok(ExitCode::SUCCESS)
        }
        "rotate-key" => {
            println!("{}", open()?.rotate_key(&arg(0)?).map_err(|e| e.message)?);
            eprintln!("The previous key no longer works. Save the new key securely.");
            Ok(ExitCode::SUCCESS)
        }
        "revoke-key" => {
            open()?.revoke_key(&arg(0)?).map_err(|e| e.message)?;
            println!("Key revoked.");
            Ok(ExitCode::SUCCESS)
        }
        "import-community" => {
            let apply = take_flag(&mut args, "--apply");
            let skip_conflicts = take_flag(&mut args, "--skip-conflicts");
            let summary_only = take_flag(&mut args, "--summary");
            let fetch = take_flag(&mut args, "--fetch");
            let any_address = take_flag(&mut args, "--allow-loopback");
            let url = take(&mut args, "--url")?;
            let retire_after = match take(&mut args, "--retire-after")? {
                None => 3,
                Some(n) => n
                    .parse::<u32>()
                    .map_err(|e| format!("--retire-after {n}: {e}"))?,
            };
            let xml = match (fetch || url.is_some(), args.as_slice()) {
                (true, []) => {
                    let url = url.as_deref().unwrap_or(community::COMMUNITY_LIST_URL);
                    let rule: dereth_network_hub::fetch::AddressRule = if any_address {
                        dereth_network_hub::discord::any_address
                    } else {
                        dereth_network_hub::fetch::is_public
                    };
                    community::fetch_list(url, rule)
                        .map_err(|e| format!("community import: {e}; nothing changed"))?
                }
                (false, [path]) => {
                    std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?
                }
                _ => {
                    return Err(format!(
                        "import-community: a file, or --fetch, and no more\n\n{USAGE}"
                    ))
                }
            };
            let input = community::parse(&xml)
                .map_err(|e| format!("community import: {e}; nothing changed"))?;
            if apply && input.entries.is_empty() && input.errors.is_empty() {
                // An empty list is far likelier a fault than every world gone at once.
                return Err("community import: the list holds no records; nothing changed".into());
            }
            if !input.errors.is_empty() && !skip_conflicts {
                for (n, name, why) in &input.errors {
                    eprintln!("record {n} ({name}): {why}");
                }
                return Err("Some records are invalid. Nothing was imported; correct the file and preview again.".into());
            }
            if !summary_only {
                for (n, name, why) in &input.errors {
                    eprintln!("record {n} ({name}): refused: {why}");
                }
                for (n, name, what) in &input.warnings {
                    eprintln!("record {n} ({name}): {what}");
                }
            }
            let options = ImportOptions {
                apply,
                skip_conflicts,
                retire_after: (retire_after > 0).then_some(retire_after),
                also_on_list: input.refused_ids.clone(),
            };
            let plan = open()?
                .import_community_with(&input.entries, &options)
                .map_err(|e| e.message)?;
            let line = community::summary(&plan, &input.errors);
            if summary_only {
                println!("{}{line}", if apply { "" } else { "dry run: " });
            } else {
                println!(
                    "{}",
                    pretty(&serde_json::json!({ "applied": apply, "plan": plan }))
                );
                eprintln!("{line}");
            }
            Ok(ExitCode::SUCCESS)
        }
        "export-community" => {
            let all = open()?.all().map_err(|e| e.message)?;
            print!("{}", dereth_network_hub::community::thwarg_xml(&all));
            Ok(ExitCode::SUCCESS)
        }
        "audit" => {
            println!(
                "{}",
                pretty(&open()?.audit(&arg(0)?).map_err(|e| e.message)?)
            );
            Ok(ExitCode::SUCCESS)
        }
        "self-update" => self_update(&mut args, &db),
        "backup" => {
            let to = arg(0)?;
            open()?.backup(&to).map_err(|e| e.message)?;
            println!("{db} copied to {to}");
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
}

/// `self-update --check|--apply [--policy patch|minor] [--source <url>]`, for the registry `db`.
fn self_update(args: &mut Vec<String>, db: &str) -> Result<ExitCode, String> {
    use dereth_network_hub::self_update::{self as update, Policy, Source, Updater};
    let usage = "self-update --check|--apply [--policy patch|minor] [--source <url>]";
    let apply = match (take_flag(args, "--check"), take_flag(args, "--apply")) {
        (true, false) => false,
        (false, true) => true,
        _ => return Err(format!("{usage}: give --check or --apply")),
    };
    let policy = match take(args, "--policy")? {
        None => Policy::Minor,
        Some(p) => Policy::parse(&p).ok_or_else(|| format!("--policy {p}: patch or minor"))?,
    };
    let source = Source::parse(
        take(args, "--source")?
            .as_deref()
            .unwrap_or(update::SOURCE_URL),
    )?;
    if !args.is_empty() {
        return Err(format!("{usage}: unexpected {args:?}"));
    }
    let updater = Updater::for_this_binary(source, std::path::Path::new(db))?;
    let check = updater.check(policy)?;
    for line in update::describe(&updater, &check) {
        println!("{line}");
    }
    let nothing = ExitCode::from(update::NOTHING_TO_INSTALL);
    if !apply {
        return Ok(if check.selection.take.is_some() {
            ExitCode::SUCCESS
        } else {
            nothing
        });
    }
    match updater.apply(&check, &mut |line| println!("{line}"))? {
        None => Ok(nothing),
        Some(installed) => {
            println!(
                "Installed {} {} as {}; the previous binary is {}, and the registry's copy {}.",
                update::PROGRAM,
                installed.version,
                updater.binary.display(),
                installed.previous.display(),
                installed.backup.display()
            );
            println!("Restart the hub to run it.");
            Ok(ExitCode::SUCCESS)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: hub: `--version` names the program and the version the release's tag carries.
    #[test]
    fn the_version_line_is_the_program_and_its_version() {
        assert_eq!(
            version_line(),
            format!("dereth-network-hub {}", env!("CARGO_PKG_VERSION"))
        );
        assert!(run(vec!["--version".into()]).is_ok());
    }

    /// Spec: hub: `end-sessions` ends every session in the registry, as a restore needs.
    #[test]
    fn end_sessions_ends_every_session_in_the_registry() {
        let dir =
            std::env::temp_dir().join(format!("dereth-hub-end-sessions-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let db = dir.join("hub.sqlite");
        let _ = std::fs::remove_file(&db);
        let db = db.to_str().expect("a path").to_owned();
        let tokens: Vec<String> = {
            let r = Registry::open(&db, "dereth.network", Box::new(std::time::SystemTime::now))
                .expect("a registry");
            ["100", "200"]
                .iter()
                .map(|d| {
                    let op = r.signed_in(d, "Someone").expect("signed in");
                    r.new_session(&op).expect("a session")
                })
                .collect()
        };
        assert!(run(vec!["end-sessions".into(), "--db".into(), db.clone()]).is_ok());
        let r = Registry::open(&db, "dereth.network", Box::new(std::time::SystemTime::now))
            .expect("a registry");
        for t in &tokens {
            assert_eq!(r.session(t).expect("asked"), None, "a session ended");
        }
        drop(r);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Spec: hub: `verify` on the command line vouches for an operator's world, recording the
    /// evidence, and refuses evidence too short to be any.
    #[test]
    fn verify_on_the_command_line_vouches_for_a_world() {
        assert!(USAGE.contains("verify <slug>"));
        let dir =
            std::env::temp_dir().join(format!("dereth-hub-cli-verify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        let db = dir.join("hub.sqlite").to_str().expect("a path").to_owned();
        {
            let r = Registry::open(&db, "dereth.network", Box::new(std::time::SystemTime::now))
                .expect("a registry");
            let op = r.signed_in("100", "Someone").expect("signed in");
            let world = Registration::from_json(&serde_json::json!({
                "slug": "vouched", "name": "Vouched", "ruleset": "PvE",
                "endpoint": { "mode": "manual", "address": "play.vouched.example", "port": 9000 },
                "emulator": { "family": "ACE" },
            }))
            .expect("a registration");
            r.register_for(&op, &world).expect("registered");
        }
        let s = |v: &[&str]| v.iter().map(|a| (*a).to_owned()).collect::<Vec<String>>();
        assert!(run(s(&["verify", "vouched", "short", "--db", &db])).is_err());
        run(s(&[
            "verify",
            "vouched",
            "Saw it running on a call with its operator.",
            "--db",
            &db,
        ]))
        .expect("vouched");
        let r = Registry::open(&db, "dereth.network", Box::new(std::time::SystemTime::now))
            .expect("a registry");
        let shown = r.server("vouched").expect("read").expect("there");
        assert_eq!(shown["ownership"]["state"], "managed", "{shown}");
        drop(r);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Spec: hub: `register` on the command line puts a world under the hub's own domain, but
    /// never at the host of a `--site-origin` it is given.
    #[test]
    fn register_on_the_command_line_uses_the_domain_but_not_the_sites_host() {
        let dir =
            std::env::temp_dir().join(format!("dereth-hub-cli-domain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        let db = dir.join("hub.sqlite").to_str().expect("a path").to_owned();
        let register = |slug: &str, address: &str| {
            let file = dir.join(format!("{slug}.json"));
            let world = serde_json::json!({
                "slug": slug, "name": "Test World", "ruleset": "PvE",
                "endpoint": { "mode": "manual", "address": address, "port": 9000 },
                "emulator": { "family": "ACE" },
            });
            std::fs::write(&file, world.to_string()).expect("written");
            run([
                "register",
                file.to_str().expect("a path"),
                "--db",
                &db,
                "--site-origin",
                "https://play.test",
            ]
            .map(str::to_owned)
            .to_vec())
        };
        assert!(register("test-one", "test1.dereth.network").is_ok());
        assert!(register("refused", "play.test").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A community list of the worlds `names`, each with an id of its own.
    fn list_of(names: &[&str]) -> String {
        let items: String = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                format!(
                    "<ServerItem><id>00000000-0000-4000-8000-{i:012}</id><name>{name}</name>\
                     <server_host>192.0.2.10</server_host><server_port>9000</server_port>\
                     <type>PvE</type></ServerItem>"
                )
            })
            .collect();
        format!("<?xml version=\"1.0\" encoding=\"utf-8\"?><ArrayOfServerItem>{items}</ArrayOfServerItem>")
    }

    /// A stand-in for the list's host on loopback: answers one request with `status` and `body`.
    fn serve_once(status: u16, body: Vec<u8>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("an address");
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut head = Vec::new();
                let mut b = [0u8; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut b) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&b[..n]),
                    }
                }
                let _ = write!(
                    s,
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(&body);
            }
        });
        format!("http://{addr}/Servers.xml")
    }

    /// Spec: hub: a fetched import whose list does not arrive or does not read changes nothing
    /// and counts as no run; one that reads counts.
    #[test]
    fn a_fetch_that_fails_changes_nothing() {
        let dir =
            std::env::temp_dir().join(format!("dereth-hub-community-fetch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let db = dir.join("hub.sqlite");
        let _ = std::fs::remove_file(&db);
        let db = db.to_str().expect("a path").to_owned();
        let file = dir.join("Servers.xml");
        std::fs::write(&file, list_of(&["Alpha", "Beta"])).expect("written");
        let s = |v: &[&str]| v.iter().map(|a| (*a).to_owned()).collect::<Vec<String>>();
        run(s(&[
            "import-community",
            file.to_str().expect("a path"),
            "--apply",
            "--db",
            &db,
        ]))
        .expect("imported");
        let fetched = |url: &str| {
            run(s(&[
                "import-community",
                "--fetch",
                "--url",
                url,
                "--allow-loopback",
                "--apply",
                "--skip-conflicts",
                "--retire-after",
                "1",
                "--summary",
                "--db",
                &db,
            ]))
        };
        let alpha_only = list_of(&["Alpha"]).into_bytes();
        let mut too_long = list_of(&["Alpha"]).into_bytes();
        too_long.resize(community::MAX_LIST_BYTES + 1, b' ');
        let dead = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            format!("http://{}/Servers.xml", l.local_addr().expect("an address"))
        };
        for (what, url) in [
            ("an error status", serve_once(500, alpha_only.clone())),
            ("a list too long", serve_once(200, too_long)),
            (
                "not a list",
                serve_once(200, b"<html>moved</html>".to_vec()),
            ),
            (
                "an empty list",
                serve_once(200, b"<ArrayOfServerItem/>".to_vec()),
            ),
            ("no answer", dead),
        ] {
            let e = fetched(&url).expect_err(what);
            assert!(e.contains("nothing changed"), "{what}: {e}");
        }
        let beta = || {
            Registry::open(&db, "dereth.network", Box::new(std::time::SystemTime::now))
                .expect("a registry")
                .public_server("beta")
                .expect("asked")
        };
        let kept = beta().expect("still listed after five failed runs");
        assert!(kept["provenance"].is_object());
        fetched(&serve_once(200, alpha_only)).expect("a run");
        assert!(beta().is_none(), "the run that read counted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Spec: hub: an address other than loopback is refused unless a TLS proxy is named or the
    /// operator accepts keys in the clear.
    #[test]
    fn a_listen_address_other_than_loopback_needs_a_proxy_or_the_operators_word() {
        let lo: std::net::SocketAddr = "127.0.0.1:8650".parse().expect("an address");
        let any: std::net::SocketAddr = "0.0.0.0:8650".parse().expect("an address");
        assert!(check_listen(lo, false, false).is_ok());
        assert!(check_listen(any, false, false).is_err());
        assert!(check_listen(any, true, false).is_ok());
        assert!(check_listen(any, false, true).is_ok());
    }

    /// Spec: hub: a hub refuses to start on a registry whose schema is newer than the one it
    /// reads, so a binary put back after an update that moved the schema forward does not run on
    /// it.
    #[test]
    fn serve_refuses_a_registry_newer_than_it() {
        let dir = std::env::temp_dir().join(format!("dereth-hub-cli-newer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        let db = dir.join("hub.sqlite").to_str().expect("a path").to_owned();
        drop(
            Registry::open(&db, "dereth.network", Box::new(std::time::SystemTime::now))
                .expect("a registry"),
        );
        let c = rusqlite::Connection::open(&db).expect("opened");
        c.pragma_update(
            None,
            "user_version",
            dereth_network_hub::registry::SCHEMA_VERSION + 1,
        )
        .expect("a newer schema");
        drop(c);
        let s = |v: &[&str]| v.iter().map(|a| (*a).to_owned()).collect::<Vec<String>>();
        let e = run(s(&[
            "serve",
            "--listen",
            "127.0.0.1:0",
            "--run-for",
            "0",
            "--db",
            &db,
        ]))
        .expect_err("refused");
        assert!(e.contains("newer than this hub"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Spec: hub: `self-update` exits 3 when no release is to be installed, with `--check` and
    /// with `--apply` alike, and refuses a command without exactly one of them or with a policy
    /// other than `patch` or `minor`.
    #[test]
    fn self_update_exits_3_when_there_is_nothing_to_install() {
        let s = |v: &[&str]| v.iter().map(|a| (*a).to_owned()).collect::<Vec<String>>();
        for bad in [
            &["self-update"][..],
            &["self-update", "--check", "--apply"],
            &["self-update", "--check", "--policy", "major"],
            &[
                "self-update",
                "--check",
                "--source",
                "https://gitlab.com/a/b",
            ],
        ] {
            assert!(run(s(bad)).is_err(), "{bad:?}");
        }
        let dir =
            std::env::temp_dir().join(format!("dereth-hub-cli-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        let db = dir.join("hub.sqlite").to_str().expect("a path").to_owned();
        for mode in ["--check", "--apply"] {
            let older = format!(
                r#"[{{"tag_name": "network-hub-v0.0.1", "assets": []}}, {{"tag_name": "network-hub-v{}", "assets": []}}]"#,
                env!("CARGO_PKG_VERSION")
            );
            let url = serve_once(200, older.into_bytes());
            let mirror = url.replace("/Servers.xml", "/repos/someone/hub");
            let code = run(s(&[
                "self-update",
                mode,
                "--policy",
                "patch",
                "--source",
                &mirror,
                "--db",
                &db,
            ]))
            .expect("checked");
            assert_eq!(code, ExitCode::from(3), "{mode}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
