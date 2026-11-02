//! `dereth-web-relay`: see the library's documentation, and `README.md` beside it.

use dereth_web_relay::config::{parse_args, Mode, USAGE};
use dereth_web_relay::limits::system_clock;
use dereth_web_relay::log::Log;
use dereth_web_relay::relay::Relay;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = parse_args(&args, &|name| std::env::var(name).ok()).unwrap_or_else(|e| {
        eprintln!("{e}\n{USAGE}");
        std::process::exit(2);
    });
    let announce = match &config.mode {
        Mode::Public(p) => p.announce.clone(),
        Mode::Local { .. } => None,
    };
    let log = Log::stderr();
    let relay = Relay::bind(config, system_clock(), log.clone()).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    log.line(&relay.describe());
    if let Some(a) = announce {
        a.spawn(log);
    }
    relay.serve();
}
