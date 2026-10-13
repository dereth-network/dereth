//! `dereth-network-conformance`: run the vectors, or the hub checks against a running hub.

use std::process::ExitCode;

use dereth_network_conformance::{failures, hub, vectors, Check};

const USAGE: &str = "\
dereth-network-conformance -- the Dereth Network's conformance suite

  dereth-network-conformance vectors
      the address, text-rule, message, challenge, directory, reach, say and rooms vectors, against
      this build's protocol crate

  dereth-network-conformance hub --url ws://127.0.0.1:8650 --world <slug>=<key> --world <slug>=<key>
                                 [--world <slug>=<key>] [--off <slug>=<key>] [--slow]
      the hub checks: two federated worlds (a third adds the room checks for a world that does not
      carry the room), optionally a world whose federation flag is not on, and --slow for the
      checks that wait out the hub's timers. The directory's checks read the hub's public HTTP
      routes at the same address, and list characters of their own, taking them out again

The world checks need a world's own adapter (`world::WorldUnderTest`); a world runs them in its
own tests.";

fn pair(s: &str) -> Result<(String, String), String> {
    s.split_once('=')
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
        .ok_or_else(|| format!("{s}: <slug>=<key>"))
}

fn report(checks: &[Check]) -> ExitCode {
    for c in checks {
        println!("{c}");
    }
    let failed = failures(checks).len();
    println!("\n{} passed, {failed} failed", checks.len() - failed);
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("vectors") => report(&vectors::all()),
        Some("hub") => {
            let mut target = hub::HubTarget {
                base: String::new(),
                worlds: Vec::new(),
                off: None,
                slow: false,
                clock: None,
            };
            let mut i = 1;
            while i < args.len() {
                let value = args.get(i + 1).cloned().unwrap_or_default();
                let step = match args[i].as_str() {
                    "--url" => {
                        target.base = value;
                        2
                    }
                    "--world" => match pair(&value) {
                        Ok(p) => {
                            target.worlds.push(p);
                            2
                        }
                        Err(e) => {
                            eprintln!("{e}");
                            return ExitCode::FAILURE;
                        }
                    },
                    "--off" => match pair(&value) {
                        Ok(p) => {
                            target.off = Some(p);
                            2
                        }
                        Err(e) => {
                            eprintln!("{e}");
                            return ExitCode::FAILURE;
                        }
                    },
                    "--slow" => {
                        target.slow = true;
                        1
                    }
                    other => {
                        eprintln!("unknown {other:?}\n\n{USAGE}");
                        return ExitCode::FAILURE;
                    }
                };
                i += step;
            }
            if target.base.is_empty() {
                eprintln!("--url is needed\n\n{USAGE}");
                return ExitCode::FAILURE;
            }
            report(&hub::run(&target))
        }
        _ => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
    }
}
