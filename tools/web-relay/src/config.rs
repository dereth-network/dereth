//! The command line.
//!
//! ```text
//! dereth-web-relay --server <host:port> [--listen 127.0.0.1:9180] [--allow-origin <origin>]
//!
//! dereth-web-relay --server <host:port> --public [--listen <addr:port>]
//!     [--allow-origin <origin>]... [--tls-cert <pem> --tls-key <pem>]
//!     [--trusted-proxy <addr or cidr>]... [--client-address-header x-forwarded-for|cf-connecting-ip]
//!     [--proxy-hops <n>] [--max-per-ip <n>] [--max-sessions <n>]
//!     [--public-url <wss url> --world <slug> [--hub <url>] [--key-file <path>]]
//! ```
//!
//! The first is a player's, on their own machine. The second, `--public`, is an operator's, beside
//! their server for many players: see [`Public`].

use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::path::PathBuf;

use crate::announce::{Announce, Key};
use crate::client::{Header, Proxies};
use crate::limits::Limits;
use crate::origin::{page_origin, WEB_CLIENT};

/// Where the relay listens unless told otherwise.
pub const DEFAULT_LISTEN: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), 9180);

/// The environment variable that holds the world's registry key, as for the network's ACE mod.
pub const KEY_VARIABLE: &str = "DERETH_NETWORK_KEY";

/// The usage line.
pub const USAGE: &str = "usage: dereth-web-relay --server <host:port> [--listen 127.0.0.1:9180] [--allow-origin <origin>]
       dereth-web-relay --server <host:port> --public [--listen <addr:port>] [--allow-origin <origin>]...
           [--tls-cert <pem> --tls-key <pem>] [--trusted-proxy <addr or cidr>]...
           [--client-address-header x-forwarded-for|cf-connecting-ip] [--proxy-hops <n>]
           [--max-per-ip <n>] [--max-sessions <n>]
           [--public-url <wss url> --world <slug> [--hub <url>] [--key-file <path>]]";

#[derive(Debug, Clone)]
pub struct Config {
    pub listen: SocketAddr,
    pub server: SocketAddr,
    pub mode: Mode,
}

#[derive(Debug, Clone)]
pub enum Mode {
    /// On the player's machine: loopback only, local pages and the one the player names.
    Local { allow_origin: Option<String> },
    /// Beside a server, for many players.
    Public(Box<Public>),
}

/// A relay an operator runs beside their server.
///
/// - It listens where the operator says, loopback by default (behind a proxy on the same
///   machine); any other address only with TLS of its own or a trusted proxy in front, since a
///   browser reaches another machine only over `wss://`.
/// - It admits only the page origins named: the Dereth web client's and the operator's.
/// - It counts sessions by client address and in total, and limits what each sends.
/// - It may announce its public address to the hub.
#[derive(Debug, Clone)]
pub struct Public {
    pub origins: Vec<String>,
    /// The certificate chain and key, PEM.
    pub tls: Option<(PathBuf, PathBuf)>,
    pub proxies: Proxies,
    pub limits: Limits,
    pub announce: Option<Announce>,
}

impl Config {
    /// The limits the relay keeps.
    #[must_use]
    pub fn limits(&self) -> Limits {
        match &self.mode {
            Mode::Local { .. } => Limits::LOCAL,
            Mode::Public(p) => p.limits,
        }
    }

    /// The proxies the relay trusts: none on the player's machine.
    #[must_use]
    pub fn proxies(&self) -> Proxies {
        match &self.mode {
            Mode::Local { .. } => Proxies::default(),
            Mode::Public(p) => p.proxies.clone(),
        }
    }
}

/// The flags only a relay beside a server takes.
const PUBLIC_ONLY: &[&str] = &[
    "--tls-cert",
    "--tls-key",
    "--trusted-proxy",
    "--client-address-header",
    "--proxy-hops",
    "--max-per-ip",
    "--max-sessions",
    "--public-url",
    "--world",
    "--hub",
    "--key-file",
];

/// The refusal for a key given on the command line, which every user of the machine can read.
const KEY_ON_THE_COMMAND_LINE: &str = "a registry key is never given on the command line, where \
    every user of the machine can read it: put it in a file named by --key-file, or in the \
    DERETH_NETWORK_KEY environment variable";

/// Reads the command line. `env` reads an environment variable (the key's).
///
/// # Errors
/// Why the command line cannot be run, never repeating a key given on it.
pub fn parse_args(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Config, String> {
    if args.iter().any(|a| looks_like_a_key(a)) {
        return Err(KEY_ON_THE_COMMAND_LINE.to_owned());
    }
    let mut listen = None;
    let mut server = None;
    let mut public = false;
    let mut origins: Vec<String> = Vec::new();
    let mut tls_cert = None;
    let mut tls_key = None;
    let mut proxies = Proxies::default();
    let mut limits = Limits::PUBLIC;
    let mut public_url = None;
    let mut world = None;
    let mut hub = None;
    let mut key_file = None;
    let mut seen_public_only = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let flag = a.as_str();
        if flag == "--public" {
            public = true;
            continue;
        }
        if matches!(flag, "--key" | "--hub-key" | "--registry-key") {
            return Err(KEY_ON_THE_COMMAND_LINE.to_owned());
        }
        let value = it
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?
            .as_str();
        if PUBLIC_ONLY.contains(&flag) {
            seen_public_only.get_or_insert(flag);
        }
        let count = |v: &str| -> Result<u32, String> {
            v.parse::<u32>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("{flag} {v}: a whole number from 1"))
        };
        match flag {
            "--listen" => listen = Some(value.parse().map_err(|e| format!("{flag}: {e}"))?),
            "--server" => server = Some(resolve_server(value).map_err(|e| format!("{flag}: {e}"))?),
            "--allow-origin" => {
                origins.push(page_origin(value).map_err(|e| format!("{flag}: {e}"))?)
            }
            "--tls-cert" => tls_cert = Some(PathBuf::from(value)),
            "--tls-key" => tls_key = Some(PathBuf::from(value)),
            "--trusted-proxy" => proxies
                .trusted
                .push(value.parse().map_err(|e| format!("{flag}: {e}"))?),
            "--client-address-header" => {
                proxies.header = value
                    .parse::<Header>()
                    .map_err(|e| format!("{flag}: {e}"))?;
            }
            "--proxy-hops" => {
                proxies.hops = usize::try_from(count(value)?).unwrap_or(1).min(8);
            }
            "--max-per-ip" => limits.per_ip = Some(count(value)?),
            "--max-sessions" => limits.sessions = Some(count(value)?),
            "--public-url" => public_url = Some(value.to_owned()),
            "--world" => world = Some(value.to_owned()),
            "--hub" => hub = Some(value.to_owned()),
            "--key-file" => key_file = Some(PathBuf::from(value)),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let server = server.ok_or("--server is required")?;

    if !public {
        if let Some(flag) = seen_public_only {
            return Err(format!(
                "{flag} is for a relay beside a server, run with --public"
            ));
        }
        let listen = listen.unwrap_or(DEFAULT_LISTEN);
        if !listen.ip().is_loopback() {
            return Err(format!(
                "--listen {listen}: a player's relay listens on loopback only (an operator's \
                 relay beside a server is run with --public)"
            ));
        }
        if origins.len() > 1 {
            return Err(
                "--allow-origin: a player's relay admits one page origin besides local ones"
                    .to_owned(),
            );
        }
        return Ok(Config {
            listen,
            server,
            mode: Mode::Local {
                allow_origin: origins.pop(),
            },
        });
    }

    let listen = listen.unwrap_or(DEFAULT_LISTEN);
    let tls = match (tls_cert, tls_key) {
        (Some(c), Some(k)) => Some((c, k)),
        (None, None) => None,
        _ => return Err("--tls-cert and --tls-key go together".to_owned()),
    };
    if !listen.ip().is_loopback() && tls.is_none() && proxies.trusted.is_empty() {
        return Err(format!(
            "--listen {listen}: a browser reaches a relay on another machine only over wss://; \
             give --tls-cert and --tls-key, or put a TLS proxy in front and name it with \
             --trusted-proxy"
        ));
    }
    if !origins.iter().any(|o| o.eq_ignore_ascii_case(WEB_CLIENT)) {
        origins.insert(0, WEB_CLIENT.to_owned());
    }
    let announce = match (world, public_url) {
        (None, None) => {
            if hub.is_some() || key_file.is_some() {
                return Err("--hub and --key-file go with --world and --public-url".to_owned());
            }
            None
        }
        (Some(world), Some(url)) => Some(Announce::new(
            hub.as_deref().unwrap_or(crate::announce::NETWORK_HUB),
            &world,
            &url,
            read_key(key_file.as_deref(), env)?,
        )?),
        _ => {
            return Err(
                "--world and --public-url go together: the world's slug at the hub, and the \
                 wss:// address players reach this relay at"
                    .to_owned(),
            )
        }
    };
    Ok(Config {
        listen,
        server,
        mode: Mode::Public(Box::new(Public {
            origins,
            tls,
            proxies,
            limits,
            announce,
        })),
    })
}

/// Whether a command-line argument holds a registry key (`drth_` and its 43 characters).
fn looks_like_a_key(arg: &str) -> bool {
    arg.to_ascii_lowercase().contains("drth_")
}

/// The world's key: from `file`, else from [`KEY_VARIABLE`].
fn read_key(
    file: Option<&std::path::Path>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Key, String> {
    let text = match file {
        Some(f) => {
            std::fs::read_to_string(f).map_err(|e| format!("--key-file {}: {e}", f.display()))?
        }
        None => env(KEY_VARIABLE).ok_or_else(|| {
            format!("--world: the world's key, in a file named by --key-file or in {KEY_VARIABLE}")
        })?,
    };
    Key::new(text.trim()).ok_or_else(|| {
        "the world's key is drth_ and 43 more characters; the one given is not".to_owned()
    })
}

/// The server as `host:port`, where the host is an address or a name. A name is looked up once,
/// at start, and its first IPv4 address kept: the game's protocol carries a server address as four
/// bytes, so an IPv6-only name cannot be a server.
///
/// # Errors
/// A malformed value or a name that does not resolve.
pub fn resolve_server(spec: &str) -> Result<SocketAddr, String> {
    if let Ok(addr) = spec.parse::<SocketAddr>() {
        return Ok(addr);
    }
    let (host, port) = spec
        .rsplit_once(':')
        .ok_or_else(|| format!("{spec}: give the server as host:port"))?;
    let port: u16 = port.parse().map_err(|e| format!("{spec}: port: {e}"))?;
    (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("{spec}: {e}"))?
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| format!("{spec}: no IPv4 address"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "drth_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    fn parse(s: &str) -> Result<Config, String> {
        parse_args(&args(s), &|_| None)
    }

    fn public(c: &Config) -> &Public {
        match &c.mode {
            Mode::Public(p) => p,
            Mode::Local { .. } => panic!("a player's relay"),
        }
    }

    /// A player's relay listens on loopback only; any other address needs `--public`, and even
    /// then only with TLS of its own or a trusted proxy in front.
    #[test]
    fn it_listens_beyond_loopback_only_when_run_beside_a_server() {
        let local = parse("--server 127.0.0.1:9000").expect("a player's relay");
        assert_eq!(local.listen, DEFAULT_LISTEN);
        assert!(matches!(local.mode, Mode::Local { .. }));
        assert!(parse("--server 127.0.0.1:9000 --listen 0.0.0.0:9180").is_err());
        assert!(parse("--server 127.0.0.1:9000 --listen 192.0.2.5:9180").is_err());
        assert!(parse("--listen 127.0.0.1:1").is_err(), "no server");

        let behind = parse("--server 127.0.0.1:9000 --public").expect("behind a local proxy");
        assert_eq!(
            behind.listen, DEFAULT_LISTEN,
            "loopback unless told otherwise"
        );
        assert!(
            parse("--server 127.0.0.1:9000 --public --listen 0.0.0.0:9180").is_err(),
            "plain ws:// on a public address serves no browser"
        );
        let tls = parse(
            "--server 127.0.0.1:9000 --public --listen 0.0.0.0:9180 --tls-cert c.pem --tls-key k.pem",
        )
        .expect("its own TLS");
        assert_eq!(tls.listen, "0.0.0.0:9180".parse().expect("an address"));
        assert!(parse(
            "--server 127.0.0.1:9000 --public --listen 10.0.0.2:9180 --trusted-proxy 10.0.0.1"
        )
        .is_ok());
        assert!(parse("--server 127.0.0.1:9000 --public --tls-cert c.pem").is_err());
    }

    /// The operator's flags are refused without `--public`, so a player's relay never takes them
    /// by mistake.
    #[test]
    fn the_operators_flags_need_public() {
        for flag in [
            "--tls-cert c.pem",
            "--trusted-proxy 127.0.0.1",
            "--max-per-ip 2",
            "--max-sessions 5",
            "--world coldeve",
        ] {
            let err = parse(&format!("--server 127.0.0.1:9000 {flag}")).expect_err(flag);
            assert!(err.contains("--public"), "{flag}: {err}");
        }
    }

    /// A player's relay admits exactly one named origin; a relay beside a server admits the Dereth
    /// web client's and every origin its operator names.
    #[test]
    fn the_origins_admitted_are_the_web_clients_and_those_named() {
        let local =
            parse("--server 127.0.0.1:9000 --allow-origin https://play.example.org/").expect("ok");
        assert!(
            matches!(&local.mode, Mode::Local { allow_origin: Some(o) } if o == "https://play.example.org")
        );
        assert!(parse(
            "--server 127.0.0.1:9000 --allow-origin https://a.example --allow-origin https://b.example"
        )
        .is_err());
        assert!(parse("--server 127.0.0.1:9000 --allow-origin https://a.example/play").is_err());

        let p = parse(
            "--server 127.0.0.1:9000 --public --allow-origin https://a.example --allow-origin http://127.0.0.1:8080",
        )
        .expect("ok");
        assert_eq!(
            public(&p).origins,
            [
                "https://play.dereth.network",
                "https://a.example",
                "http://127.0.0.1:8080"
            ]
        );
        let plain = parse("--server 127.0.0.1:9000 --public").expect("ok");
        assert_eq!(public(&plain).origins, ["https://play.dereth.network"]);
    }

    #[test]
    fn the_limits_are_counts_from_one() {
        let p =
            parse("--server 127.0.0.1:9000 --public --max-per-ip 2 --max-sessions 40").expect("ok");
        assert_eq!(public(&p).limits.per_ip, Some(2));
        assert_eq!(public(&p).limits.sessions, Some(40));
        assert_eq!(
            public(&parse("--server 127.0.0.1:9000 --public").expect("ok")).limits,
            Limits::PUBLIC
        );
        assert!(parse("--server 127.0.0.1:9000 --public --max-per-ip 0").is_err());
        assert!(parse("--server 127.0.0.1:9000 --public --max-sessions lots").is_err());
    }

    /// The server may be named: `localhost` resolves to its IPv4 address, an address is taken as it
    /// is, and a name without a port is refused.
    #[test]
    fn the_server_may_be_given_by_name() {
        assert_eq!(
            resolve_server("localhost:9000"),
            Ok("127.0.0.1:9000".parse().expect("an address"))
        );
        assert_eq!(
            resolve_server("10.0.0.5:9000"),
            Ok("10.0.0.5:9000".parse().expect("an address"))
        );
        assert!(resolve_server("localhost").is_err());
    }

    /// A key on the command line is refused, in any flag or position, and the refusal does not
    /// repeat it.
    #[test]
    fn a_key_is_never_taken_from_the_command_line() {
        for line in [
            format!("--server 127.0.0.1:9000 --public --key {KEY}"),
            "--server 127.0.0.1:9000 --public --hub-key whatever".to_owned(),
            format!("--server 127.0.0.1:9000 --public --world coldeve --public-url wss://r.example.org/ --key-file {KEY}"),
            format!("--server {KEY}"),
            format!("--server 127.0.0.1:9000 --hub=https://x.example/?{KEY}"),
        ] {
            let err = parse(&line).expect_err(&line);
            assert!(!err.contains(KEY) && !err.contains("drth_A"), "{err}");
            assert!(err.contains("never given on the command line"), "{err}");
        }
    }

    /// The key comes from a file or the environment, and a relay told to announce without one is
    /// refused.
    #[test]
    fn the_key_comes_from_a_file_or_the_environment() {
        let line = args(
            "--server 127.0.0.1:9000 --public --world coldeve --public-url wss://relay.example.org/",
        );
        let env = |name: &str| (name == KEY_VARIABLE).then(|| format!("{KEY}\n"));
        let c = parse_args(&line, &env).expect("from the environment");
        let a = public(&c).announce.as_ref().expect("announces");
        assert_eq!(a.key().expose(), KEY);
        assert!(
            !format!("{c:?}").contains(KEY),
            "the key is not in its Debug"
        );
        let err = parse_args(&line, &|_| None).expect_err("no key");
        assert!(
            err.contains("--key-file") && err.contains(KEY_VARIABLE),
            "{err}"
        );

        let dir = std::env::temp_dir().join(format!("dereth-web-relay-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let file = dir.join("key");
        std::fs::write(&file, format!("{KEY}\r\n")).expect("written");
        let mut with_file = line.clone();
        with_file.extend(["--key-file".to_owned(), file.display().to_string()]);
        let c = parse_args(&with_file, &|_| None).expect("from the file");
        assert_eq!(
            public(&c)
                .announce
                .as_ref()
                .expect("announces")
                .key()
                .expose(),
            KEY
        );
        std::fs::write(&file, "not a key").expect("written");
        let err = parse_args(&with_file, &|_| None).expect_err("a bad key");
        assert!(!err.contains("not a key"), "{err}");
        std::fs::remove_dir_all(&dir).expect("removed");

        assert!(
            parse_args(
                &args("--server 127.0.0.1:9000 --public --world coldeve"),
                &env
            )
            .is_err(),
            "no public URL"
        );
    }
}
