//! What `yalain --help` prints: every option, and the world database's commands.

/// The help, printed to standard output by `--help` and `-h`.
pub const TEXT: &str = "\
yalain --dats <install folder> [options]

  --dats DIR            the September 2013 install the server reads its data from (a copy no
                        client is running from: a client opens its data files alone)
  --bind IP             the address to listen on (default 127.0.0.1)
  --login-port P        the login service's even port (default 9000; P + 1 is used too)
  --world-port P        the world service's even port (default 9002; P + 1 is used too)
  --advertise IP        the IPv4 address players from elsewhere are sent to for the world;
                        players on this computer or its network get the nearby address
                        (default: --bind; needed when --bind is 0.0.0.0 or an IPv6 address)
  --data FILE           the database of accounts and characters (default yalain.sqlite)
  --memory              keep nothing: a database in memory
  --world-name NAME     the world's name, shown at login (default \"Local World\")
  --listed-accounts     admit only accounts made with --add-account (default: any new name
                        makes its account, with its ticket)
  --banned-names FILE   refuse character names holding these words: one pattern a line, `*`
                        for any run of letters (default: none)
  --add-account N:T     make account N with ticket T, then carry on
  --admin N             make account N an admin's when this start makes it (--add-account),
                        then carry on
  --admin-command W:G,G keep the account-level admin command W to the named groups G
  --serve-admin-library serve admin accounts' retail clients the script library that sends
                        their commands in the clear
  --secret-cost N       the bcrypt cost account secrets are kept at, 4 to 31 (default 8)
  --save-interval S     keep each character in the world every S seconds (default 300)
  --watchdog S          stop with a failure if the world runs no pass for S seconds, 0 for never
                        (default 300)
  --shutdown-interval S how many seconds a shutdown counts down, 0 to 99999 (default 120)
  --max-sessions N      how many logins the world holds at once (default 128); a login past
                        them is told the server is full
  --min-character-age S  seconds a character must have played before it sends to the
                        Dereth Network: tells, room lines, channels (default 0)
  --max-sessions-per-address N  how many logins one address holds at once (default
                        `unlimited`)
  --unlimited-sessions-from IP  an address no per-address limit holds back (may be given
                        many times)
  --download-rate N     the bytes a second a client is sent a large data download at, 4096 to
                        262144 (default 65536, the retail servers' rate)
  --motd TEXT           a message shown to each player as its character enters the world
                        (`\\n` starts a new line)
  --add-character A:N:R:S  make character N on account A of race R and sex S (4096 or 8192),
                        with a middling physique, then carry on; a missing account is made on
                        an open world only
  --place A:N:CELL:X:Y:Z:H  put character N of account A at a place (cell in hex, a heading
                        in degrees) before it next enters, then carry on
  --money A:N:AMOUNT    set the money of character N of account A (one that has played) before
                        it next enters, then carry on
  --make-character FILE make the character a recipe file describes, with its build (a level or
                        experience, skills, money, items, quests, a hero, a place), before it
                        first plays, then carry on
  --world-db DIR        the world database to serve, compiled against --dats at start: a
                        folder, or `pinned` (the default) for the release this build pins,
                        fetched into the per-user cache as it starts if it is not there; a world
                        that names a base database (`[base]`, as Athenaeum's do) is served
                        over that release in the cache
  --world-base DIR      the base database a --world-db world lies over, in place of the
                        release it names in the cache
  --no-referral         carry the world on the login connection instead of referring the
                        client to the world service
  --no-echo             do not send players their own positions back
  --game-time S         start the game clock at S seconds since the relaunch's start (1
                        December 2012; default: now); a day of the game is 7,347 seconds
  --hold-clock          hold the game clock where it starts: the sky and the time of day stand
                        still, for comparing clients at one moment; what the game counts in its
                        own time (a skill's recovery, the wait after a recall, a quest's wait)
                        does not run out while it is held
  --hub URL             report this world to the Dereth Network hub at URL (https://, or
                        http:// to this machine), with --hub-slug and --hub-key-file: the
                        hub's launcher lists it among Asheron's Call 2's worlds
  --hub-slug SLUG       the world's slug at the hub, as it was registered
  --hub-key-file FILE   the file holding the key the hub gave the world (drth_...)
  --federation          link the world to the hub's network (with --hub), once the world's
                        federation is on at the hub: characters whose players put them on the
                        network reach and are reached by other worlds' characters, of either
                        game
  --global-room ROOM    the room that carries the network's Global room until the hub keeps
                        one: general (the default), trade, lfg, roleplay, or empty for none
  --no-friends          serve no friends service: by default the world serves its players'
                        friends, presence and tells to Name@world on TCP at the login port's
                        number
  --seconds S           stop after S seconds (default: run until stopped)
  --verbose             log at the debug level (every connection event, and more)
  --stop-at-console-end stop, keeping everyone in the world, when the console's input ends:
                        for a program that runs the server and holds its console (by
                        default the end of the input ends only the console)
  --update POLICY       which new releases the server installs by itself: off (the default),
                        patch or minor; yalain update --check says what would be taken
  --update-check-hours H  hours between its checks for a new release (default 6)
  --update-warning-seconds S  the countdown players see before it restarts into a new release
                        (default 300)
  --update-source URL   where new releases are looked for (default: the GitHub repository this
                        build came from)
  --status IP:PORT      answer GET /health and GET /status (the world's state, as JSON) over
                        HTTP at this address (default: off); keep it on loopback or a private
                        network: it has no authentication
  --no-status-ping      answer no status ping: by default the login port tells a launcher or
                        the Dereth Network's check the world's state, players, version and
                        name, with no login
  --status-ping-hellos N  status ping first steps a minute from one address (default 4)
  --status-ping-asks N  status ping second steps a minute from one address (default 4)
  --status-ping-replies N  status ping replies a second, to everyone (default 50)
  --websocket           also take WebSocket connections, for clients in a browser, as
                        Empyrean's `[server.websocket]` does, with the options below; off by
                        default
  --websocket-listen IP:PORT  where (default 0.0.0.0:9443)
  --websocket-tls-certificate FILE, --websocket-tls-private-key FILE  the PEM files for wss://;
                        without them plain ws:// is served, on loopback or with
                        --websocket-behind-tls-proxy only
  --websocket-behind-tls-proxy  a reverse proxy in front terminates TLS
  --websocket-trusted-proxy IP  a proxy whose X-Forwarded-For names the client (may be given
                        many times)
  --websocket-allowed-origin URL  a web page allowed to connect, by origin (may be given many
                        times); none refuses every page
  --websocket-max-per-address N  connections open at once from one address (default 4; -1 is
                        unlimited)
  --websocket-idle-timeout S  seconds a connection may send nothing (default 60)
  --websocket-public-url URL  the URL clients use, reported to the status endpoint and the hub
                        (default: from --websocket-listen)
  --config FILE         read the settings from this configuration file, in the form of
                        Empyrean's empyrean.toml (default: yalain.toml in the working folder,
                        else beside the executable, if there is one); the command line wins
                        over it

yalain world check|release|fetch|compile ...

  the world database's commands: yalain world check <folder>, yalain world fetch, and the rest

yalain update --check|--apply [--config <yalain.toml>] [--policy <off|patch|minor>]

  says what a new release would involve and why each newer one would or would not be installed
  (--check), or installs it on a stopped server and leaves it stopped (--apply)

yalain --version

  print the server's version and stop

yalain --release-facts

  print, as one line of JSON, the version, the target, the database's schema version and the
  world database release this build serves, for a launcher's updates, and stop

Exit codes: 0 stopped as asked; 1 failed; 3 a port the server listens on is held by another
program (its log's line ends in [ports-taken]).
";

#[cfg(test)]
mod tests {
    //! Behaviour: none (the executable's help: every option it lists is one the command line takes)
    use super::TEXT;

    /// The options the help lists: each line's first word that starts with `--`.
    fn listed() -> Vec<&'static str> {
        TEXT.lines()
            .filter_map(|l| l.split_whitespace().next())
            .filter(|w| w.starts_with("--"))
            .collect()
    }

    #[test]
    fn every_option_the_help_lists_is_one_the_command_line_takes() {
        let options = listed();
        assert!(options.len() >= 25, "{options:?}");
        for o in options {
            let got = crate::parse_from([o.to_owned()]);
            if let Err(e) = got {
                assert!(!e.starts_with("unknown option"), "{o}: {e}");
            }
        }
    }

    #[test]
    fn help_and_h_ask_for_the_help() {
        for o in ["--help", "-h"] {
            assert_eq!(
                crate::parse_from([o.to_owned()]).err().as_deref(),
                Some(""),
                "{o}"
            );
        }
        assert!(TEXT.contains("log at the debug level (every connection event, and more)"));
        assert!(TEXT.contains("yalain --version"));
        assert!(TEXT.contains("yalain --release-facts"));
        assert!(TEXT.contains("3 a port the server listens on is held"));
    }
}
