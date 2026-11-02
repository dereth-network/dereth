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
  --secret-cost N       the bcrypt cost account secrets are kept at, 4 to 31 (default 8)
  --save-interval S     keep each character in the world every S seconds (default 300)
  --shutdown-interval S how many seconds a shutdown counts down, 0 to 99999 (default 120)
  --max-sessions N      how many logins the world holds at once (default 128); a login past
                        them is told the server is full
  --max-sessions-per-address N  how many logins one address holds at once (default
                        `unlimited`)
  --unlimited-sessions-from IP  an address no per-address limit holds back (may be given
                        many times)
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
                        fetched into the per-user cache with `yalain world fetch`; a world
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
  --no-friends          serve no friends service: by default the world serves its players'
                        friends, presence and tells to Name@world on TCP at the login port's
                        number
  --seconds S           stop after S seconds (default: run until stopped)
  --verbose             log at the debug level (every connection event, and more)
  --stop-at-console-end stop, keeping everyone in the world, when the console's input ends:
                        for a program that runs the server and holds its console (by
                        default the end of the input ends only the console)
  --status IP:PORT      answer GET /health and GET /status (the world's state, as JSON) over
                        HTTP at this address (default: off); keep it on loopback or a private
                        network: it has no authentication
  --no-status-ping      answer no status ping: by default the login port tells a launcher or
                        the Dereth Network's check the world's state, players, version and
                        name, with no login
  --status-ping-hellos N  status ping first steps a minute from one address (default 4)
  --status-ping-asks N  status ping second steps a minute from one address (default 4)
  --status-ping-replies N  status ping replies a second, to everyone (default 50)
  --config FILE         read the settings from this configuration file, in the form of
                        Empyrean's empyrean.toml (default: yalain.toml in the working folder,
                        else beside the executable, if there is one); the command line wins
                        over it

yalain world check|release|fetch|compile ...

  the world database's commands: yalain world check <folder>, yalain world fetch, and the rest

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
