//! What `yalain --help` prints: every option, and the world database's commands.

/// The help, printed to standard output by `--help` and `-h`.
pub const TEXT: &str = "\
yalain --dats <install folder> [options]

  --dats DIR            the September 2013 install the server reads its data from (a copy no
                        client is running from: a client opens its data files alone)
  --bind IP             the address to listen on (default 127.0.0.1)
  --login-port P        the login service's even port (default 9000; P + 1 is used too)
  --world-port P        the world service's even port (default 9002; P + 1 is used too)
  --advertise IP        the IPv4 address a referral sends clients to (default: --bind; needed
                        when --bind is 0.0.0.0 or an IPv6 address)
  --data FILE           the database of accounts and characters (default yalain.sqlite)
  --memory              keep nothing: a database in memory
  --world-name NAME     the world's name, shown at login (default \"Local World\")
  --listed-accounts     admit only accounts made with --add-account (default: any new name
                        makes its account, with its ticket)
  --banned-names FILE   refuse character names holding these words: one pattern a line, `*`
                        for any run of letters (default: none)
  --add-account N:T     make account N with ticket T, then carry on
  --add-character A:N:R:S  make character N on account A of race R and sex S (4096 or 8192),
                        with a middling physique, then carry on; a missing account is made on
                        an open world only
  --place A:N:CELL:X:Y:Z:H  put character N of account A at a place (cell in hex, a heading
                        in degrees) before it next enters, then carry on
  --money A:N:AMOUNT    set the money of character N of account A (one that has played) before
                        it next enters, then carry on
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
  --seconds S           stop after S seconds (default: run until stopped)
  --verbose             log at the debug level (every connection event, and more)

yalain world check|release|fetch|compile ...

  the world database's commands: yalain world check <folder>, yalain world fetch, and the rest

yalain --version

  print the server's version and stop
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
        assert!(options.len() >= 20, "{options:?}");
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
    }
}
