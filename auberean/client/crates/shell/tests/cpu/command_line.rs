//! The command line as the September 2013 client reads it.

use auberean_shell::command_line::{convert_command_line, parse_command_line, ClientLine};

/// An option is named by a word starting with `-` or `/`: doubled, its long name; one letter after
/// it, its letter (else the long name); otherwise its long name. Each keeps its value by its form:
/// the port as a number in any base, the account in small letters, a switch's own value; an
/// option taking a value takes the next word even when it looks like an option. Some switches set
/// preferences for the run; options not named keep the client's own (the server's port 7304, a
/// window of 1024 by 768, the friends service's port 2310). A line is split at spaces outside double
/// quotes.
///
/// Behaviour: desktop.command-line.the-clients-options-read-as-retail-reads-them
#[test]
fn the_clients_options_read_as_retail_reads_them() {
    let (line, ok) = ClientLine::parse(&[
        "ac2client.exe",
        "-h",
        "127.0.0.1",
        "--port",
        "0x2328",
        "/account",
        "MyAcct",
        "-z",
        "secret",
        "-u",
        "Rowan",
        "-gunserver",
        "gun.example",
        "--gunport",
        "2310",
        "//gunticket",
        "-ff",
        "--nolb",
        "-maxgore",
        "LOW",
        "-ls",
    ]);
    assert!(ok, "{}", line.output);
    assert_eq!(line.host, "127.0.0.1");
    assert_eq!(line.port, 9000);
    assert_eq!(line.account, "myacct", "in small letters");
    assert_eq!(line.ticket.as_deref(), Some("secret"));
    assert_eq!(line.user.as_deref(), Some("Rowan"));
    assert_eq!(line.gun_server.as_deref(), Some("gun.example"));
    assert_eq!((line.gun_port, line.gun_ticket), (2310, 1));
    assert_eq!(
        line.run_values,
        [(0x4000_0002, 0x4000_000D, 1), (0x4000_0004, 0x4000_0014, 0)]
    );
    assert_eq!(line.top_gore, Some(3));
    assert_eq!(line.ignored, ["ls"]);
    assert!(line.output.is_empty() && !line.shown);

    let (taken, ok) = ClientLine::parse(&["x", "-a", "acct", "-h", "-p"]);
    assert!(ok);
    assert_eq!(
        (taken.host.as_str(), taken.port),
        ("-p", 7304),
        "the next word, whatever it is; the port left at the client's own"
    );
    assert_eq!(
        (taken.width, taken.height, taken.gun_port),
        (1024, 768, 2310),
        "the client's own"
    );

    let words = convert_command_line("ac2client.exe -h \"my host\" -a acct \"\" -width 800")
        .expect("quotes closed");
    assert_eq!(
        words,
        [
            "ac2client.exe",
            "-h",
            "my host",
            "-a",
            "acct",
            "-width",
            "800"
        ]
    );
    assert_eq!(convert_command_line("-h \"open"), None);
    let mut line = ClientLine::default();
    assert!(parse_command_line(
        &mut line,
        "ac2client.exe -h \"my host\" -a acct -width 800",
        true
    ));
    assert_eq!((line.host.as_str(), line.width), ("my host", 800));
}

/// A line that does not read writes out why, a blank line, then every option's usage, sorted by
/// long name: the letter and the long name padded to 20 columns, `: `, the help wrapped at 80
/// columns with its next lines indented 22. An unknown option, a loose word, a value missing at the
/// end, a bad gore level each say so; `-?` shows the usage alone. Read whole, the line still needs
/// an account and a host, and the friends service's ticket needs the service.
///
/// Behaviour: desktop.command-line.a-line-that-does-not-read-shows-why-and-the-usage
#[test]
fn a_line_that_does_not_read_shows_why_and_the_usage() {
    let fails = |words: &[&str]| {
        let (line, ok) = ClientLine::parse(words);
        assert!(!ok && line.shown, "{words:?}");
        line.output
    };
    let out = fails(&["x", "-q"]);
    assert!(
        out.starts_with("Unrecognized argument -q\n\n-a, --account       : <string>: "),
        "{out}"
    );
    assert!(
        out.contains("\n    --autonomy      : <int>: Specify the level of autonomy. 0,1 or 2.\n")
    );
    assert!(out.contains(
        "\n    --nodbwlib      : Tells the client not to use the database cache for \n                      \
         locating the wlib.  Instead, the wlib will be loaded \n                      \
         directly from the hard drive.\n"
    ), "{out}");
    let names: Vec<&str> = out
        .lines()
        .filter_map(|l| l.split("--").nth(1)?.split_whitespace().next())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_by_key(|n| n.to_ascii_lowercase());
    assert_eq!(names, sorted);
    assert_eq!(names.len(), 50);
    assert!(fails(&["x", "-a", "acct", "-h"]).starts_with("Argument -h requires parameter\n\n"));
    assert!(fails(&["x", "loose"]).starts_with("Argument 'loose' unknown\n\n"));
    assert!(
        fails(&["x", "-?"]).starts_with("-a, --account"),
        "no message"
    );
    assert!(fails(&["x", "-maxgore", "lots"]).starts_with("gore level must be one of these"));
    assert!(fails(&["x", "-h", "host"]).starts_with("You must specify an account name\n\n"));
    assert!(fails(&["x", "-a", "acct"]).starts_with("You must specify a host name\n\n"));
    assert!(fails(&["x", "-a", "a", "-h", "h", "--gunticket"])
        .starts_with("You must specify a gun server in order to use GUN ticketing\n\n"));
}
