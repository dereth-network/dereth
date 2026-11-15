//! Behaviour: none (tooling: how a release's change log is posted to Discord, pinned and linked to
//! the one before it, against a channel held in memory).

use super::*;

const BOT: &str = "900";
const CHANNEL: &str = "42";
const URL: &str = "https://github.com/dereth-network/dereth/releases/tag/dereth-v0.4.19";

/// A channel in memory, recording every call that changes it.
#[derive(Debug, Default)]
struct Channel {
    /// Every message, in the order posted.
    messages: Vec<Pinned>,
    /// The pinned ones' ids, the most recently pinned first.
    pinned: Vec<String>,
    /// "post", "pin <id>" and "unpin <id>", in order.
    calls: Vec<String>,
    /// The call that fails, by its name ("pins", "post", "pin", "unpin").
    fail: Option<&'static str>,
}

impl Channel {
    fn with_pinned(posts: &[(&str, &str, &str)]) -> Self {
        let mut c = Self::default();
        // given most recent first, pinned in the opposite order
        for (id, author, content) in posts.iter().rev() {
            c.messages.push(Pinned {
                id: (*id).to_owned(),
                author: (*author).to_owned(),
                content: (*content).to_owned(),
            });
            c.pinned.insert(0, (*id).to_owned());
        }
        c
    }

    fn failing(&self, call: &str) -> Result<(), String> {
        if self.fail == Some(call) {
            Err(format!("{call}: Discord answered 500: no"))
        } else {
            Ok(())
        }
    }

    fn content(&self, id: &str) -> &str {
        &self
            .messages
            .iter()
            .find(|m| m.id == id)
            .expect("a message")
            .content
    }
}

impl Discord for Channel {
    fn me(&mut self) -> Result<String, String> {
        Ok(BOT.to_owned())
    }

    fn guild(&mut self, channel: &str) -> Result<Option<String>, String> {
        assert_eq!(channel, CHANNEL);
        Ok(Some("7".to_owned()))
    }

    fn pins(&mut self, channel: &str) -> Result<Vec<Pinned>, String> {
        assert_eq!(channel, CHANNEL);
        self.failing("pins")?;
        Ok(self
            .pinned
            .iter()
            .map(|id| {
                self.messages
                    .iter()
                    .find(|m| &m.id == id)
                    .expect("pinned")
                    .clone()
            })
            .collect())
    }

    fn post(&mut self, channel: &str, content: &str) -> Result<String, String> {
        assert_eq!(channel, CHANNEL);
        self.failing("post")?;
        assert!(content.chars().count() <= MESSAGE_LIMIT);
        let id = format!("m{}", self.messages.len() + 1);
        self.messages.push(Pinned {
            id: id.clone(),
            author: BOT.to_owned(),
            content: content.to_owned(),
        });
        self.calls.push("post".to_owned());
        Ok(id)
    }

    fn pin(&mut self, channel: &str, message: &str) -> Result<(), String> {
        assert_eq!(channel, CHANNEL);
        self.failing("pin")?;
        self.pinned.insert(0, message.to_owned());
        self.calls.push(format!("pin {message}"));
        Ok(())
    }

    fn unpin(&mut self, channel: &str, message: &str) -> Result<(), String> {
        assert_eq!(channel, CHANNEL);
        self.failing("unpin")?;
        self.pinned.retain(|m| m != message);
        self.calls.push(format!("unpin {message}"));
        Ok(())
    }
}

fn run(c: &mut Channel, version: &str, highlights: Option<&str>) -> Result<Outcome, String> {
    announce(c, CHANNEL, Product::Dereth, version, highlights, URL)
}

/// The post is the heading with the product and version, the highlights with each wrapped line
/// joined, the release's link without a preview, and the previous post's link.
#[test]
fn a_post_is_the_heading_the_highlights_and_the_two_links() {
    let highlights = "- Web: new render scale option.\n- Fixed players showing up in the wrong\n  place after taking a portal.";
    let text = post_text(
        Product::Dereth,
        "0.4.19",
        Some(highlights),
        URL,
        Some("https://discord.com/channels/7/42/5"),
    );
    assert_eq!(
        text,
        "## Dereth 0.4.19\n\
         - Web: new render scale option.\n\
         - Fixed players showing up in the wrong place after taking a portal.\n\
         \n\
         Release: <https://github.com/dereth-network/dereth/releases/tag/dereth-v0.4.19>\n\
         Previous: https://discord.com/channels/7/42/5"
    );
    // The first release has no previous post, and an empty section points at the release.
    let first = post_text(Product::Lyceum, "0.3.0", Some("  \n"), URL, None);
    assert_eq!(
        first,
        format!("## Lyceum 0.3.0\nSee the release for what changed.\n\nRelease: <{URL}>")
    );
}

/// A post's heading names its product and version; another product's, a heading of something
/// else, or no version, is not one of the product's posts.
#[test]
fn a_posts_heading_names_its_product_and_version() {
    assert_eq!(
        version_of(Product::Dereth, "## Dereth 0.4.19\n- a"),
        Some("0.4.19")
    );
    assert_eq!(version_of(Product::Dereth, "## Lyceum 0.4.19\n- a"), None);
    assert_eq!(version_of(Product::Dereth, "## Dereth news\n- a"), None);
    assert_eq!(version_of(Product::Dereth, "Dereth 0.4.19"), None);
    assert_eq!(
        version_of(Product::Yalain, "## Yalain 0.4.1"),
        Some("0.4.1")
    );
}

/// A release tag names its product by its prefix; the web client's, the network's and an unknown
/// tag name none.
#[test]
fn a_release_tag_names_its_product_and_version() {
    assert_eq!(
        product_of_tag("dereth-v0.4.19"),
        Some((Product::Dereth, "0.4.19"))
    );
    assert_eq!(
        product_of_tag("empyrean-v0.3.14"),
        Some((Product::Empyrean, "0.3.14"))
    );
    assert_eq!(
        product_of_tag("launcher-v1.0.0"),
        Some((Product::Launcher, "1.0.0"))
    );
    assert_eq!(
        product_of_tag("athenaeum-v0.2.0"),
        Some((Product::Athenaeum, "0.2.0"))
    );
    for none in [
        "dereth-web-v0.4.19",
        "network-hub-v0.3.9",
        "network-ace-v0.1.5",
        "dereth-v",
        "v1",
    ] {
        assert_eq!(product_of_tag(none), None, "{none}");
    }
    assert_eq!(channel_var(Product::Athenaeum), "DISCORD_CHANNEL_ATHENAEUM");
}

/// The command takes a product and a version or a tag; a tag of a product without a change log is
/// nothing to announce, and a bad version or a product without a change log is refused.
#[test]
fn the_command_takes_a_product_and_version_or_a_tag() {
    let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let r = parse(&args(&[
        "yalain",
        "0.4.1",
        "--release-url",
        "https://x/r",
        "--dry-run",
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(
        r,
        Request {
            product: Product::Yalain,
            version: "0.4.1".to_owned(),
            release_url: Some("https://x/r".to_owned()),
            dry_run: true,
        }
    );
    let r = parse(&args(&["--tag", "lyceum-v0.3.0"])).unwrap().unwrap();
    assert_eq!((r.product, r.version.as_str()), (Product::Lyceum, "0.3.0"));
    assert_eq!(
        parse(&args(&["--tag", "network-hub-v0.3.9"])).unwrap(),
        None
    );
    assert!(parse(&args(&["dereth", "four"])).is_err());
    assert!(parse(&args(&["network-hub", "0.3.9"])).is_err());
    assert!(parse(&args(&["dereth"])).is_err());
    assert!(parse(&args(&["--tag", "dereth-v1.0.0", "dereth", "1.0.0"])).is_err());
}

/// A long post is split between lines into messages Discord takes, and a line longer than a whole
/// message within itself; nothing is lost.
#[test]
fn a_long_post_is_split_between_lines_into_messages_discord_takes() {
    let lines: Vec<String> = (0..300).map(|i| format!("- change number {i}")).collect();
    let text = lines.join("\n");
    let parts = split(&text, MESSAGE_LIMIT);
    assert!(parts.len() > 1);
    assert!(parts.iter().all(|p| p.chars().count() <= MESSAGE_LIMIT));
    assert!(
        parts.iter().all(|p| p.starts_with("- change number")),
        "split between lines"
    );
    assert_eq!(parts.join("\n"), text);

    let long = "é".repeat(4500);
    let parts = split(&long, MESSAGE_LIMIT);
    assert_eq!(
        parts.iter().map(|p| p.chars().count()).collect::<Vec<_>>(),
        [2000, 2000, 500]
    );
    assert_eq!(parts.concat(), long);
    assert_eq!(split("short\n\nlines", MESSAGE_LIMIT), ["short\n\nlines"]);
}

/// The first release is posted and pinned, with no previous link.
#[test]
fn the_first_release_is_posted_and_pinned() {
    let mut c = Channel::default();
    let out = run(&mut c, "0.4.19", Some("- One thing.")).unwrap();
    assert_eq!(
        out,
        Outcome::Posted {
            id: "m1".to_owned(),
            unpinned: Vec::new()
        }
    );
    assert_eq!(c.calls, ["post", "pin m1"]);
    assert!(!c.content("m1").contains("Previous:"));
    assert_eq!(c.pinned, ["m1"]);
}

/// The next release links the pinned post before it, is pinned before that one is unpinned, and
/// leaves only itself pinned among the product's posts; others' pins and other products' posts
/// stay pinned.
#[test]
fn the_next_release_links_the_previous_post_pins_itself_then_unpins_it() {
    let mut c = Channel::with_pinned(&[
        ("5", BOT, "## Lyceum 0.3.0\n- a"),
        ("4", BOT, "## Dereth 0.4.18\n- b"),
        ("3", "111", "## Dereth 0.4.17 (someone's own note)"),
        ("2", BOT, "## Dereth 0.4.16\n- c"),
        ("1", "111", "Welcome to the channel"),
    ]);
    let out = run(&mut c, "0.4.19", Some("- Next.")).unwrap();
    assert_eq!(
        out,
        Outcome::Posted {
            id: "m6".to_owned(),
            unpinned: vec!["4".to_owned(), "2".to_owned()]
        }
    );
    assert_eq!(c.calls, ["post", "pin m6", "unpin 4", "unpin 2"]);
    assert!(c
        .content("m6")
        .ends_with("\nPrevious: https://discord.com/channels/7/42/4"));
    assert_eq!(c.pinned, ["m6", "5", "3", "1"]);
}

/// Run again for a version already pinned, or an older one, it posts nothing and changes nothing.
#[test]
fn a_rerun_or_an_older_release_posts_nothing() {
    let mut c = Channel::with_pinned(&[("4", BOT, "## Dereth 0.4.19\n- b")]);
    for version in ["0.4.19", "0.4.2"] {
        let out = run(&mut c, version, Some("- x")).unwrap();
        assert_eq!(
            out,
            Outcome::AlreadyAnnounced {
                id: "4".to_owned(),
                version: "0.4.19".to_owned()
            },
            "{version}"
        );
    }
    assert!(c.calls.is_empty());
    // A newer patch release than the one pinned is posted (0.4.20 is after 0.4.19).
    assert!(matches!(
        run(&mut c, "0.4.20", None).unwrap(),
        Outcome::Posted { .. }
    ));
}

/// A long change log goes as several messages, and its first, the one with the heading, is pinned.
#[test]
fn a_long_change_log_pins_its_first_message() {
    let highlights: Vec<String> = (0..200).map(|i| format!("- change number {i}")).collect();
    let mut c = Channel::with_pinned(&[("4", BOT, "## Dereth 0.4.18\n- b")]);
    let out = run(&mut c, "0.4.19", Some(&highlights.join("\n"))).unwrap();
    assert_eq!(
        out,
        Outcome::Posted {
            id: "m2".to_owned(),
            unpinned: vec!["4".to_owned()]
        }
    );
    let posts = c.calls.iter().filter(|c| *c == "post").count();
    assert!(posts > 1);
    assert_eq!(c.calls[posts..], ["pin m2", "unpin 4"]);
    assert!(c.content("m2").starts_with("## Dereth 0.4.19\n"));
    // The rerun finds the pinned first message and posts nothing.
    let before = c.calls.len();
    assert!(matches!(
        run(&mut c, "0.4.19", None).unwrap(),
        Outcome::AlreadyAnnounced { .. }
    ));
    assert_eq!(c.calls.len(), before);
}

/// A Discord error stops the steps after it and comes back as an error: a failed pin leaves the
/// old post pinned, and nothing is unpinned.
#[test]
fn a_discord_error_stops_before_anything_is_unpinned() {
    let mut c = Channel::with_pinned(&[("4", BOT, "## Dereth 0.4.18\n- b")]);
    c.fail = Some("pin");
    let e = run(&mut c, "0.4.19", None).unwrap_err();
    assert!(e.contains("500"), "{e}");
    assert_eq!(c.calls, ["post"]);
    assert_eq!(c.pinned, ["4"]);

    let mut c = Channel {
        fail: Some("pins"),
        ..Channel::default()
    };
    assert!(run(&mut c, "0.4.19", None).is_err());
    assert!(
        c.calls.is_empty(),
        "nothing posted when the pins cannot be read"
    );
}

/// The token never shows in a debug print.
#[test]
fn the_token_never_shows_in_a_debug_print() {
    let https = Https::new(Token("secret-value".to_owned()));
    assert!(!format!("{https:?}").contains("secret-value"));
}
