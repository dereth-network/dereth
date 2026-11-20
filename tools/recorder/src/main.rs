//! `dereth-recorder`: see the library's documentation, and `README.md` beside it.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dereth_pcap::flows::PortSet;
use dereth_recorder::read::{describe_sessions, line, parse_id, read_file};
use dereth_recorder::record::{server_address, Options, Recorder, DEFAULT_LISTEN, DEFAULT_SERVER};

const USAGE: &str = "\
usage:
  dereth-recorder record --out <file.pcap> [--server HOST:PORT] [--listen 127.0.0.1:PORT]
      Listen on loopback (default 127.0.0.1:9100, and the port above it) and pass everything on
      to the server (default 127.0.0.1:9000, and the port above it), writing each datagram to
      the file, which must not exist yet. Point the client at the listening address. Ctrl-C
      stops it.
  dereth-recorder read <file.pcap> [--object ID] [--server-ports 9000-9003]
      List every message of a recording, or with --object only those that name that object,
      in order, with the time, the session, the direction, the type and its name.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("record") => record(&args[1..]),
        Some("read") => read(&args[1..]),
        _ => Err(USAGE.to_owned()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

/// The value after each of `names`, and the words that are no option.
fn options<'a>(
    args: &'a [String],
    names: &[&str],
) -> Result<(Vec<Option<&'a str>>, Vec<&'a str>), String> {
    let mut values = vec![None; names.len()];
    let mut words = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if let Some(i) = names.iter().position(|n| n == a) {
            let v = it
                .next()
                .ok_or_else(|| format!("{a} needs a value\n{USAGE}"))?;
            values[i] = Some(v.as_str());
        } else if a.starts_with("--") {
            return Err(format!("unknown option {a}\n{USAGE}"));
        } else {
            words.push(a.as_str());
        }
    }
    Ok((values, words))
}

fn record(args: &[String]) -> Result<(), String> {
    let (v, words) = options(args, &["--out", "--server", "--listen"])?;
    if let Some(w) = words.first() {
        return Err(format!("unexpected {w:?}\n{USAGE}"));
    }
    let out = PathBuf::from(v[0].ok_or_else(|| format!("record needs --out <file>\n{USAGE}"))?);
    let server = server_address(v[1].unwrap_or(DEFAULT_SERVER))?;
    let listen_text = v[2].unwrap_or(DEFAULT_LISTEN);
    let listen: SocketAddr = listen_text
        .parse()
        .map_err(|e| format!("--listen {listen_text}: {e} (want 127.0.0.1:PORT)"))?;

    let recorder = Recorder::start(&Options {
        listen,
        server,
        out: out.clone(),
    })?;
    let at = recorder.listen();
    println!("recording to {}", out.display());
    println!(
        "listening on {at} and {}, passing on to {server} and {}",
        at.port() + 1,
        server.port() + 1
    );
    println!("point the client at {at}; Ctrl-C stops the recording");

    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let _ = ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst));
    let mut shown = Instant::now();
    while !stop.load(Ordering::SeqCst) && recorder.failure().is_none() {
        std::thread::sleep(Duration::from_millis(200));
        if shown.elapsed() >= Duration::from_secs(5) {
            let c = recorder.counts();
            println!(
                "  {} to the server, {} to the client",
                c.to_server, c.to_client
            );
            shown = Instant::now();
        }
    }
    let c = recorder.stop()?;
    println!(
        "recorded {} datagrams to the server and {} to the client{} in {}",
        c.to_server,
        c.to_client,
        if c.referrals > 0 {
            format!(", through {} referral(s)", c.referrals)
        } else {
            String::new()
        },
        out.display()
    );
    Ok(())
}

fn read(args: &[String]) -> Result<(), String> {
    let (v, words) = options(args, &["--object", "--server-ports"])?;
    let [file] = words[..] else {
        return Err(format!("read takes one recording\n{USAGE}"));
    };
    let ports = v[1]
        .map(|p| PortSet::parse(p).map_err(|e| format!("--server-ports {p}: {e}")))
        .transpose()?;
    let r = read_file(&PathBuf::from(file), ports)?;
    print!("{}", describe_sessions(&r));
    match v[0] {
        Some(text) => {
            let id = parse_id(text)?;
            let mut n = 0;
            for (m, how) in r.naming(id) {
                println!("{}", line(m, Some(&how)));
                n += 1;
            }
            println!("{n} messages name 0x{id:X}");
        }
        None => {
            for m in &r.messages {
                println!("{}", line(m, None));
            }
        }
    }
    Ok(())
}
