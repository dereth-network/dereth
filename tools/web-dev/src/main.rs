//! The web client's developer runner: builds the browser client and serves it on this machine.
//!
//! **Depends on** `wasm-bindgen-cli-support` (the bindgen step, as a library pinned to the web
//! client's own `wasm-bindgen`), `dereth-dat` for the retail data files' names, and `ctrlc`.
//! **Used by** developers, through `cargo xtask web` or `cargo run -p dereth-web-dev`; no crate
//! depends on it. It runs `cargo` to build the web client and, for a server that speaks only UDP,
//! the web relay.
//!
//! **Must never** listen anywhere but loopback: it serves the player's own retail data files with
//! `--dat-dir` and `--classic-dat-dir`, and those never leave this machine. It is a developer convenience, not a
//! deployment tool (the web client's `DEPLOY.md` is that).
//!
//! ```text
//! dereth-web-dev [--dev] [--no-build | --build-only] [--port 8080] [--dat-dir <dir>] [--classic-dat-dir <dir>] [--server <url | host:port>]
//! ```
//!
//! - It builds `dereth-web` for `wasm32-unknown-unknown` (the `web-release` profile, or `release`
//!   with `--dev`, which keeps function names for stack traces), runs the bindgen step into `www/pkg/`,
//!   and serves `www/` at `http://127.0.0.1:<port>/`. `--build-only` stops after the build, for a
//!   deployment.
//! - With `--dat-dir`, it also serves the retail data files from that folder at `/dats/<name>`,
//!   with byte ranges, for the worker's development reader: the four of the later set, and
//!   `portal.dat` and `cell.dat` from before Throne of Destiny when the folder holds them.
//!   `--classic-dat-dir` names another folder for those two, as the desktop client's switch does.
//! - With `--server`, it prints the page's address with the server filled in: a `ws://` or `wss://`
//!   URL is an Empyrean's WebSocket endpoint and is used as it is; a `host:port` is a server that
//!   speaks only UDP, for which it builds and starts `dereth-web-relay` on `127.0.0.1:9180`.
//!
//! The pages use no `SharedArrayBuffer`, so no cross-origin isolation headers are sent. A page
//! opened with `?echo=1` posts its log lines to `/log`, and the runner prints them.

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

/// Whether `name` is one of the retail data files the worker's development reader asks for: one of
/// the later set's four.
fn is_dat_name(name: &str) -> bool {
    dereth_dat::ModernDat::ALL
        .iter()
        .any(|d| d.file_name() == name)
}

/// Whether `name` is one of the two files of the set from before Throne of Destiny.
fn is_classic_dat_name(name: &str) -> bool {
    dereth_dat::ClassicDat::ALL
        .iter()
        .any(|d| d.file_name() == name)
}

/// Where `dereth-web-relay` listens when the runner starts it.
const RELAY_LISTEN: &str = "127.0.0.1:9180";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Server {
    /// An Empyrean's WebSocket URL, given to the page as it is.
    Url(String),
    /// A server that speaks only UDP, reached through the relay.
    Udp(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Options {
    dev: bool,
    build: bool,
    serve: bool,
    port: u16,
    dat_dir: Option<PathBuf>,
    classic_dat_dir: Option<PathBuf>,
    server: Option<Server>,
}

fn usage() -> ! {
    eprintln!(
        "usage: dereth-web-dev [--dev] [--no-build | --build-only] [--port 8080] [--dat-dir <dir>] [--classic-dat-dir <dir>] [--server <ws(s)://url | host:port>]"
    );
    std::process::exit(2);
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        dev: false,
        build: true,
        serve: true,
        port: 8080,
        dat_dir: None,
        classic_dat_dir: None,
        server: None,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--dev" => o.dev = true,
            "--no-build" => o.build = false,
            "--build-only" => o.serve = false,
            "--port" => o.port = value()?.parse().map_err(|e| format!("{a}: {e}"))?,
            "--dat-dir" => o.dat_dir = Some(PathBuf::from(value()?)),
            "--classic-dat-dir" => o.classic_dat_dir = Some(PathBuf::from(value()?)),
            "--server" => o.server = Some(server(value()?)?),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(o)
}

/// A `--server` value: a WebSocket URL, or a `host:port` for the relay.
fn server(value: &str) -> Result<Server, String> {
    if value.starts_with("ws://") || value.starts_with("wss://") {
        return Ok(Server::Url(value.to_owned()));
    }
    match value.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.parse::<u16>().is_ok() => {
            Ok(Server::Udp(value.to_owned()))
        }
        _ => Err(format!(
            "--server {value}: give a ws:// or wss:// URL, or a UDP server as host:port"
        )),
    }
}

/// The workspace root: this crate is `tools/web-dev` in it. Not canonicalised, so the paths this
/// prints read as typed (a canonical path on Windows carries the `\\?\` prefix).
fn workspace() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map_or_else(|| manifest.join("../.."), Path::to_path_buf)
}

/// Where cargo puts what it builds: `CARGO_TARGET_DIR` when it is set (relative to the workspace,
/// as cargo reads it), else the workspace's `target`.
fn target_dir(ws: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .filter(|d| !d.is_empty())
        .map_or_else(|| ws.join("target"), |d| ws.join(d))
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("{cmd:?}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{cmd:?}: {status}"))
    }
}

/// The module's compiler flags that keep the building machine's folders out of it. Panic messages
/// name their source files, and the standard library's and the registry's crates are named by
/// full paths, which hold the builder's home folder; these name them `/rustc` and `/cargo`
/// instead. The workspace's own files are already named relative to it.
fn remap_flags() -> Vec<String> {
    let mut flags = Vec::new();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    if let Ok(out) = Command::new(rustc).args(["--print", "sysroot"]).output() {
        let sysroot = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if out.status.success() && !sysroot.is_empty() {
            flags.push(format!("--remap-path-prefix={sysroot}=/rustc"));
        }
    }
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|home| PathBuf::from(home).join(".cargo"))
        });
    if let Some(home) = cargo_home {
        flags.push(format!("--remap-path-prefix={}=/cargo", home.display()));
    }
    flags
}

/// `flags` as a `--config` value for the WebAssembly target's compiler flags.
fn target_rustflags(flags: &[String]) -> String {
    let quoted: Vec<String> = flags.iter().map(|f| format!("'{f}'")).collect();
    format!(
        "target.wasm32-unknown-unknown.rustflags=[{}]",
        quoted.join(", ")
    )
}

/// Builds the module and runs the bindgen step into `www/pkg/`.
fn build(ws: &Path, dev: bool) -> Result<(), String> {
    let profile = if dev { "release" } else { "web-release" };
    run(Command::new(env!("CARGO")).current_dir(ws).args([
        "build",
        "--profile",
        profile,
        "--target",
        "wasm32-unknown-unknown",
        "-p",
        "dereth-web",
        "--config",
        &target_rustflags(&remap_flags()),
    ]))?;
    let wasm = target_dir(ws)
        .join("wasm32-unknown-unknown")
        .join(profile)
        .join("dereth_web.wasm");
    let out = ws.join("dereth").join("web").join("www").join("pkg");
    let mut bindgen = wasm_bindgen_cli_support::Bindgen::new();
    bindgen
        .input_path(&wasm)
        .web(true)
        .map_err(|e| e.to_string())?
        .typescript(false)
        // The module's own URL as the loader's default, as the command-line tool writes it: the
        // worker calls `init()` with no argument.
        .omit_default_module_path(false)
        .remove_name_section(!dev);
    bindgen
        .generate(&out)
        .map_err(|e| format!("bindgen {}: {e}", wasm.display()))
}

/// Builds and starts the relay for `server`, for a page served from this machine.
fn start_relay(ws: &Path, server: &str) -> Result<Child, String> {
    run(Command::new(env!("CARGO")).current_dir(ws).args([
        "build",
        "--release",
        "-p",
        "dereth-web-relay",
    ]))?;
    let exe = target_dir(ws)
        .join("release")
        .join(format!("dereth-web-relay{}", std::env::consts::EXE_SUFFIX));
    Command::new(&exe)
        .args(["--server", server, "--listen", RELAY_LISTEN])
        .spawn()
        .map_err(|e| format!("{}: {e}", exe.display()))
}

/// The page's address, with the data source and the server filled in.
fn page_url(port: u16, dats: bool, server: Option<&str>) -> String {
    let mut params = Vec::new();
    if dats {
        params.push("dats=http".to_owned());
    }
    if let Some(s) = server {
        params.push(format!("server={}", percent_encode(s)));
    }
    let query = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    format!("http://127.0.0.1:{port}/{query}")
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// The content type for a file under `www/`.
fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("css") => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// The folders the data files are served from: the later set's, and the older set's when it is
/// another.
#[derive(Debug, Clone, Default)]
struct DatDirs {
    later: Option<PathBuf>,
    classic: Option<PathBuf>,
}

/// What a request path names: a file under `www/`, or one of the data files. `None` for anything
/// else, and for a data file when no folder was given.
fn resolve(www: &Path, dats: &DatDirs, path: &str) -> Option<PathBuf> {
    let path = path.split('?').next().unwrap_or("");
    if let Some(name) = path.strip_prefix("/dats/") {
        // A file the folder does not hold is not found, so the page reads it as absent.
        let dir = if is_dat_name(name) {
            dats.later.as_ref()
        } else if is_classic_dat_name(name) {
            dats.classic.as_ref().or(dats.later.as_ref())
        } else {
            None
        };
        return dir.map(|d| d.join(name)).filter(|p| p.is_file());
    }
    let rel = if path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    if rel.split('/').any(|part| part == ".." || part.is_empty()) {
        return None;
    }
    let target = www.join(rel);
    let canon = target.canonicalize().ok()?;
    (canon.starts_with(www) && canon.is_file()).then_some(canon)
}

/// The byte range a `Range` header asks for within `size` bytes: `Ok(None)` for the whole file,
/// `Err` for a range that cannot be served.
fn range(header: Option<&str>, size: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(spec) = header.and_then(|h| h.strip_prefix("bytes=")) else {
        return Ok(None);
    };
    let (start, end) = spec.split_once('-').ok_or(())?;
    let start: u64 = start.parse().map_err(|_| ())?;
    let end = if end.is_empty() {
        size.saturating_sub(1)
    } else {
        end.parse::<u64>()
            .map_err(|_| ())?
            .min(size.saturating_sub(1))
    };
    if start > end || start >= size {
        return Err(());
    }
    Ok(Some((start, end)))
}

fn respond(
    stream: &mut TcpStream,
    status: &str,
    headers: &[(&str, String)],
    body: &[u8],
) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {status}\r\n");
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("Cache-Control: no-store\r\nConnection: close\r\n\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn serve_one(mut stream: TcpStream, www: &Path, dats: &DatDirs) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut range_header = None;
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("range") {
                range_header = Some(v.trim().to_owned());
            } else if k.trim().eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut parts = request.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    // The page's log, echoed here when the page is opened with `?echo=1`.
    if method == "POST" && path == "/log" {
        let mut body = vec![0u8; length.min(1 << 16)];
        reader.read_exact(&mut body)?;
        println!("page: {}", String::from_utf8_lossy(&body).trim_end());
        return respond(&mut stream, "204 No Content", &[], b"");
    }
    let head_only = method == "HEAD";
    if method != "GET" && !head_only {
        return respond(&mut stream, "405 Method Not Allowed", &[], b"");
    }
    let Some(target) = resolve(www, dats, path) else {
        return respond(&mut stream, "404 Not Found", &[], b"not found\n");
    };
    if !path.starts_with("/dats/") {
        eprintln!("{method} {path}");
    }
    let mut file = std::fs::File::open(&target)?;
    let size = file.metadata()?.len();
    let (status, start, end) = match range(range_header.as_deref(), size) {
        Ok(None) => ("200 OK", 0, size.saturating_sub(1)),
        Ok(Some((s, e))) => ("206 Partial Content", s, e),
        Err(()) => {
            return respond(
                &mut stream,
                "416 Range Not Satisfiable",
                &[("Content-Range", format!("bytes */{size}"))],
                b"",
            )
        }
    };
    let len = if size == 0 { 0 } else { end - start + 1 };
    let mut headers = vec![
        ("Content-Type", content_type(&target).to_owned()),
        ("Content-Length", len.to_string()),
        ("Accept-Ranges", "bytes".to_owned()),
    ];
    if status.starts_with("206") {
        headers.push(("Content-Range", format!("bytes {start}-{end}/{size}")));
    }
    let mut body = Vec::new();
    if !head_only && len > 0 {
        file.seek(SeekFrom::Start(start))?;
        body.resize(usize::try_from(len).unwrap_or(0), 0);
        file.read_exact(&mut body)?;
    }
    respond(&mut stream, status, &headers, &body)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "dereth-web-dev [--dev] [--no-build | --build-only] [--port 8080] [--dat-dir <dir>] [--classic-dat-dir <dir>] [--server <ws(s)://url | host:port>]\n\n\
             Builds the web client into dereth/web/www/pkg and serves dereth/web/www on 127.0.0.1\n\
             (--build-only stops after the build). --dat-dir serves the retail data files from\n\
             that folder to the page (the four client_*.dat files, and portal.dat and cell.dat from\n\
             before Throne of Destiny when it holds them; --classic-dat-dir names another folder for\n\
             those two); --server fills the page's server: a ws:// or wss:// URL as it is, a\n\
             host:port through dereth-web-relay on {RELAY_LISTEN}."
        );
        return;
    }
    let options = parse_args(&args).unwrap_or_else(|e| {
        eprintln!("{e}");
        usage()
    });
    let ws = workspace();
    let www = ws.join("dereth").join("web").join("www");
    if !www.is_dir() {
        eprintln!(
            "{} is not here: this checkout has no web client",
            www.display()
        );
        std::process::exit(1);
    }
    if options.build {
        if let Err(e) = build(&ws, options.dev) {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
    if !options.serve {
        println!("built {}", www.join("pkg").display());
        return;
    }
    let canonical = |flag: &str, d: &PathBuf| {
        d.canonicalize().unwrap_or_else(|e| {
            eprintln!("{flag} {}: {e}", d.display());
            std::process::exit(1);
        })
    };
    let dats = DatDirs {
        later: options.dat_dir.as_ref().map(|d| canonical("--dat-dir", d)),
        classic: options
            .classic_dat_dir
            .as_ref()
            .map(|d| canonical("--classic-dat-dir", d)),
    };

    let relay: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let url = match &options.server {
        Some(Server::Url(u)) => Some(u.clone()),
        Some(Server::Udp(addr)) => match start_relay(&ws, addr) {
            Ok(child) => {
                *relay
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(child);
                Some(format!("ws://{RELAY_LISTEN}/"))
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        None => None,
    };
    {
        let relay = Arc::clone(&relay);
        let _ = ctrlc::set_handler(move || {
            if let Some(mut child) = relay
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
            {
                let _ = child.kill();
                let _ = child.wait();
            }
            std::process::exit(0);
        });
    }

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, options.port));
    let listener = TcpListener::bind(addr).unwrap_or_else(|e| {
        eprintln!("{addr}: {e}");
        std::process::exit(1);
    });
    let www = www.canonicalize().expect("www");
    println!(
        "serving {}{}",
        page_url(
            options.port,
            dats.later.is_some() || dats.classic.is_some(),
            url.as_deref()
        ),
        [
            ("data files", options.dat_dir.as_ref()),
            ("older data files", options.classic_dat_dir.as_ref()),
        ]
        .iter()
        .filter_map(|(what, d)| d.map(|d| format!(" with the {what} in {}", d.display())))
        .collect::<String>()
    );
    for stream in listener.incoming().flatten() {
        let (www, dats) = (www.clone(), dats.clone());
        std::thread::spawn(move || {
            let _ = serve_one(stream, &www, &dats);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn a_websocket_url_is_used_as_it_is_and_an_address_goes_through_the_relay() {
        let o = parse_args(&args("--server wss://play.example.org/ws")).unwrap();
        assert_eq!(
            o.server,
            Some(Server::Url("wss://play.example.org/ws".to_owned()))
        );
        let o = parse_args(&args(
            "--server localhost:9000 --dev --port 8081 --classic-dat-dir old",
        ))
        .unwrap();
        assert_eq!(o.server, Some(Server::Udp("localhost:9000".to_owned())));
        assert!(o.dev && o.port == 8081);
        assert_eq!(o.classic_dat_dir, Some(PathBuf::from("old")));
        assert!(parse_args(&args("--server play.example.org")).is_err());
        assert!(parse_args(&args("--listen 0.0.0.0:8080")).is_err());
    }

    #[test]
    fn the_page_address_carries_the_data_source_and_the_server() {
        assert_eq!(page_url(8080, false, None), "http://127.0.0.1:8080/");
        assert_eq!(
            page_url(8080, true, Some("ws://127.0.0.1:9180/")),
            "http://127.0.0.1:8080/?dats=http&server=ws%3A%2F%2F127.0.0.1%3A9180%2F"
        );
    }

    #[test]
    fn only_the_four_data_files_and_files_under_www_are_served() {
        let root = std::env::temp_dir().join(format!("web-dev-{}", std::process::id()));
        let (www, dats) = (root.join("www"), root.join("dats"));
        std::fs::create_dir_all(&www).unwrap();
        std::fs::create_dir_all(&dats).unwrap();
        std::fs::write(www.join("index.html"), "<p>").unwrap();
        std::fs::write(root.join("secret.txt"), "no").unwrap();
        let portal = dereth_dat::ModernDat::Portal;
        std::fs::write(portal.in_dir(&dats), "d").unwrap();
        std::fs::write(dats.join("other.dat"), "x").unwrap();
        let www = www.canonicalize().unwrap();
        let none = DatDirs::default();
        let later = DatDirs {
            later: Some(dats.clone()),
            classic: None,
        };
        assert!(resolve(&www, &none, "/").is_some());
        assert!(resolve(&www, &none, "/index.html?x=1").is_some());
        assert!(resolve(&www, &none, "/../secret.txt").is_none());
        let url = format!("/dats/{}", portal.file_name());
        assert!(resolve(&www, &none, &url).is_none(), "no folder given");
        assert!(resolve(&www, &later, &url).is_some());
        assert!(resolve(&www, &later, "/dats/other.dat").is_none());
        assert!(resolve(&www, &later, "/dats/../secret.txt").is_none());
        // The older set's two files: from their own folder when one is named, else beside the
        // later ones, and only when they are there.
        let older = dereth_dat::ClassicDat::Portal;
        let older_url = format!("/dats/{}", older.file_name());
        assert!(
            resolve(&www, &later, &older_url).is_none(),
            "not beside them"
        );
        let classic = root.join("classic");
        std::fs::create_dir_all(&classic).unwrap();
        std::fs::write(older.in_dir(&classic), "o").unwrap();
        let both = DatDirs {
            later: Some(dats.clone()),
            classic: Some(classic.clone()),
        };
        assert_eq!(
            resolve(&www, &both, &older_url),
            Some(older.in_dir(&classic))
        );
        std::fs::write(older.in_dir(&dats), "o").unwrap();
        assert_eq!(resolve(&www, &later, &older_url), Some(older.in_dir(&dats)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_folders_to_hide_are_passed_as_the_webassembly_targets_compiler_flags() {
        let flags = vec![
            r"--remap-path-prefix=C:\Users\me\.cargo=/cargo".to_owned(),
            "--remap-path-prefix=/home/me/.rustup/toolchains/stable=/rustc".to_owned(),
        ];
        assert_eq!(
            target_rustflags(&flags),
            r"target.wasm32-unknown-unknown.rustflags=['--remap-path-prefix=C:\Users\me\.cargo=/cargo', '--remap-path-prefix=/home/me/.rustup/toolchains/stable=/rustc']"
        );
        assert_eq!(
            target_rustflags(&[]),
            "target.wasm32-unknown-unknown.rustflags=[]"
        );
    }

    #[test]
    fn a_byte_range_is_served_as_asked_and_a_bad_one_refused() {
        assert_eq!(range(None, 10), Ok(None));
        assert_eq!(range(Some("bytes=2-5"), 10), Ok(Some((2, 5))));
        assert_eq!(range(Some("bytes=2-"), 10), Ok(Some((2, 9))));
        assert_eq!(range(Some("bytes=8-100"), 10), Ok(Some((8, 9))));
        assert_eq!(range(Some("bytes=10-12"), 10), Err(()));
        assert_eq!(range(Some("bytes=5-2"), 10), Err(()));
    }
}
