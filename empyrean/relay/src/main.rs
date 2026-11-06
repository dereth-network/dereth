//! `empyrean-relay --upstream <host:port>`: run a relay on the loopback address, so a Dereth client can
//! play an ac1999 server over the later protocol.

use std::io::Write as _;
use std::net::ToSocketAddrs as _;
use std::path::PathBuf;
use std::process::ExitCode;

use empyrean_relay::relay::{Config, Finished};
use empyrean_relay::report;

const USAGE: &str = "\
empyrean-relay -- Dereth to an ac1999 server

  --upstream <host:port>  the ac1999 server (required)
  --port <port>           the game port Dereth connects to on 127.0.0.1 (default: 9000);
                          the port above it is taken too
  --world <name>          the world's name, which names its overlay folder (default: the
                          upstream address)
  --once                  stop when the first session ends
  --idle-exit <seconds>   stop when no session has run and Dereth has sent nothing for this long
  --report <file>         write what each session exchanged, by opcode
  --version               the relay's version
  -h, --help              this text";

struct Log;

impl log::Log for Log {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

static LOG: Log = Log;

fn secret() -> [u8; 32] {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).expect("the operating system's random source");
    bytes
}

fn real_main() -> Result<ExitCode, String> {
    let mut upstream = None;
    let mut port = 9000u16;
    let mut world = None;
    let mut once = false;
    let mut idle_exit = None;
    let mut report_file: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |what: &str| args.next().ok_or_else(|| format!("{what} needs a value"));
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(ExitCode::SUCCESS);
            }
            "--upstream" => upstream = Some(value("--upstream")?),
            "--port" => {
                let v = value("--port")?;
                port = v.parse().map_err(|_| format!("--port {v}: want a port"))?;
            }
            "--world" => world = Some(value("--world")?),
            "--once" => once = true,
            "--idle-exit" => {
                let v = value("--idle-exit")?;
                let seconds: u64 = v
                    .parse()
                    .map_err(|_| format!("--idle-exit {v}: want a number of seconds"))?;
                idle_exit = Some(std::time::Duration::from_secs(seconds));
            }
            "--version" => {
                println!("empyrean-relay {}", env!("CARGO_PKG_VERSION"));
                return Ok(ExitCode::SUCCESS);
            }
            "--report" => report_file = Some(PathBuf::from(value("--report")?)),
            other => return Err(format!("{other}: not an option\n\n{USAGE}")),
        }
    }
    let upstream = upstream.ok_or_else(|| format!("--upstream is required\n\n{USAGE}"))?;
    let address = upstream
        .to_socket_addrs()
        .map_err(|e| format!("--upstream {upstream}: {e}"))?
        .find(std::net::SocketAddr::is_ipv4)
        .ok_or_else(|| format!("--upstream {upstream}: no IPv4 address"))?;
    let mut seed = [0; 8];
    getrandom::fill(&mut seed).map_err(|e| e.to_string())?;
    let config = Config {
        port,
        upstream: address,
        world: world.unwrap_or(upstream),
        seed: u64::from_le_bytes(seed),
    };
    let done = |finished: &Finished| {
        let text = report::text(finished);
        eprintln!("{text}");
        if let Some(file) = &report_file {
            let written = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file)
                .and_then(|mut f| f.write_all(text.as_bytes()));
            if let Err(e) = written {
                eprintln!("--report {}: {e}", file.display());
            }
        }
        once
    };
    empyrean_relay::serve::run(config, Box::new(secret), idle_exit, done)
        .map_err(|e| e.to_string())?;
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    if log::set_logger(&LOG).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    match real_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("empyrean-relay: {e}");
            ExitCode::FAILURE
        }
    }
}
