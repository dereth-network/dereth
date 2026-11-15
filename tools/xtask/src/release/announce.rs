//! A release's change log as a post in its product's Discord channel: pinned, and linking the post
//! of the product's release before it.
//!
//! `cargo xtask announce <product> <version>` (or `--tag <tag>`) takes the release's section of the
//! product's `CHANGES.md`, the highlights `release-notes` puts first, and makes one Discord post of
//! it: a heading with the product and version, the lines, the release's link and the previous
//! post's link. It posts that to the channel `DISCORD_CHANNEL_<PRODUCT>` names, pins it, and unpins
//! the product's older posts there. The bot's token is `DISCORD_RELEASE_TOKEN`. The `announce.yml`
//! workflow runs it when a release is published; `--dry-run` prints the post and sends nothing.
//!
//! The channel's pins are the only state. A pinned post of the bot's whose heading names the product
//! is one of its change logs, the newest pinned the previous one. When one already names this
//! version, or a newer one, nothing is posted, so running it again does nothing twice. A product
//! without a channel, a run without a token, and a pre-release announce nothing.
//!
//! Announcing never fails a release: what goes wrong with Discord is a warning, and the command
//! still succeeds.

use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use super::notes;
use crate::package::version::{self, Product};
use crate::package::{self, git};

/// The variable the bot's token is read from.
pub const TOKEN_VAR: &str = "DISCORD_RELEASE_TOKEN";

/// Where Discord's API is, at the version every call uses.
pub const API: &str = "https://discord.com/api/v10";

/// The most characters one Discord message holds.
pub const MESSAGE_LIMIT: usize = 2000;

/// The products a release can be announced for: those that keep a change log.
pub const PRODUCTS: [Product; 7] = [
    Product::Dereth,
    Product::Empyrean,
    Product::Lyceum,
    Product::Launcher,
    Product::Auberean,
    Product::Yalain,
    Product::Athenaeum,
];

/// The variable naming `product`'s channel: `DISCORD_CHANNEL_DERETH`, ...
pub fn channel_var(product: Product) -> String {
    format!(
        "DISCORD_CHANNEL_{}",
        product.command_name().to_ascii_uppercase()
    )
}

/// The product and version a release tag names, when it is one of [`PRODUCTS`]' tags.
pub fn product_of_tag(tag: &str) -> Option<(Product, &str)> {
    PRODUCTS.into_iter().find_map(|p| {
        tag.strip_prefix(p.tag_prefix())
            .filter(|v| !v.is_empty())
            .map(|v| (p, v))
    })
}

/// A pinned message, as much of it as announcing reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pinned {
    pub id: String,
    /// Its author's user id.
    pub author: String,
    pub content: String,
}

/// The Discord calls announcing makes. The real one is [`Https`]; the tests hold a channel in memory.
pub trait Discord {
    /// The bot's own user id.
    fn me(&mut self) -> Result<String, String>;
    /// The server (guild) the channel is in, which a link to one of its messages names.
    fn guild(&mut self, channel: &str) -> Result<Option<String>, String>;
    /// The channel's pinned messages, the most recently pinned first.
    fn pins(&mut self, channel: &str) -> Result<Vec<Pinned>, String>;
    /// Posts `content`; the new message's id.
    fn post(&mut self, channel: &str, content: &str) -> Result<String, String>;
    fn pin(&mut self, channel: &str, message: &str) -> Result<(), String>;
    fn unpin(&mut self, channel: &str, message: &str) -> Result<(), String>;
}

/// A change log post's first line.
pub fn heading(product: Product, version: &str) -> String {
    format!("## {} {version}", product.display_name())
}

/// The version a post's heading names, when the post is one of `product`'s change logs.
pub fn version_of(product: Product, content: &str) -> Option<&str> {
    let first = content.lines().next()?;
    let v = first
        .strip_prefix("## ")?
        .strip_prefix(product.display_name())?
        .strip_prefix(' ')?
        .trim();
    version::parse_release_version(v).ok().map(|_| v)
}

/// The link that opens a message in Discord.
pub fn message_link(guild: Option<&str>, channel: &str, message: &str) -> String {
    format!(
        "https://discord.com/channels/{}/{channel}/{message}",
        guild.unwrap_or("@me")
    )
}

/// The post: the heading, the highlights with each wrapped line joined, the release's link (in
/// angle brackets, so Discord shows no preview of it), and the previous post's link.
pub fn post_text(
    product: Product,
    version: &str,
    highlights: Option<&str>,
    release_url: &str,
    previous: Option<&str>,
) -> String {
    let mut out = heading(product, version);
    out.push('\n');
    match highlights.map(str::trim).filter(|h| !h.is_empty()) {
        Some(h) => out.push_str(&notes::unwrap_lines(h)),
        None => out.push_str("See the release for what changed."),
    }
    out.push_str(&format!("\n\nRelease: <{release_url}>"));
    if let Some(p) = previous {
        out.push_str(&format!("\nPrevious: {p}"));
    }
    out
}

/// `text` as messages of at most `limit` characters each, split between lines, and within a line
/// only when the line alone is longer.
pub fn split(text: &str, limit: usize) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut part = String::new();
    for line in text.lines() {
        let mut line: Vec<char> = line.chars().collect();
        loop {
            let used = part.chars().count();
            let room = if used == 0 {
                limit
            } else {
                limit.saturating_sub(used + 1)
            };
            if line.len() <= room {
                if used > 0 {
                    part.push('\n');
                }
                part.extend(line);
                break;
            }
            if used > 0 {
                parts.push(std::mem::take(&mut part));
                continue;
            }
            let rest = line.split_off(limit);
            parts.push(line.into_iter().collect());
            line = rest;
        }
    }
    if !part.trim().is_empty() {
        parts.push(part);
    }
    parts
}

/// What announcing did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Posted: the post's first message (the one pinned), and the older posts unpinned.
    Posted { id: String, unpinned: Vec<String> },
    /// A post of the product's already pinned names this version or a newer one: nothing posted.
    AlreadyAnnounced { id: String, version: String },
}

/// Posts `product`'s change log for `version` to `channel`, pins it and unpins the product's older
/// posts there. The post is pinned before anything is unpinned, so the channel is never left with
/// none.
///
/// # Errors
/// Discord's, as text.
pub fn announce(
    d: &mut dyn Discord,
    channel: &str,
    product: Product,
    version: &str,
    highlights: Option<&str>,
    release_url: &str,
) -> Result<Outcome, String> {
    let me = d.me()?;
    let ours: Vec<(Pinned, String)> = d
        .pins(channel)?
        .into_iter()
        .filter(|p| p.author == me)
        .filter_map(|p| {
            let v = version_of(product, &p.content)?.to_owned();
            Some((p, v))
        })
        .collect();
    let numbers = |v: &str| version::parse_release_version(v).unwrap_or([0; 3]);
    let own = numbers(version);
    if let Some((p, v)) = ours.iter().find(|(_, v)| numbers(v) >= own) {
        return Ok(Outcome::AlreadyAnnounced {
            id: p.id.clone(),
            version: v.clone(),
        });
    }
    let previous = match ours.first() {
        Some((p, _)) => Some(message_link(d.guild(channel)?.as_deref(), channel, &p.id)),
        None => None,
    };
    let text = post_text(
        product,
        version,
        highlights,
        release_url,
        previous.as_deref(),
    );
    let mut first = None;
    for part in split(&text, MESSAGE_LIMIT) {
        let id = d.post(channel, &part)?;
        first.get_or_insert(id);
    }
    let id = first.ok_or("the post was empty")?;
    d.pin(channel, &id)?;
    let mut unpinned = Vec::new();
    for (p, _) in &ours {
        d.unpin(channel, &p.id)?;
        unpinned.push(p.id.clone());
    }
    Ok(Outcome::Posted { id, unpinned })
}

/// The bot's token. It goes in the `Authorization` header and nowhere else: it is never printed,
/// logged or written, and `Debug` does not show it.
pub struct Token(String);

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(hidden)")
    }
}

/// Discord over HTTPS, with rustls and the bundled web roots.
#[derive(Debug)]
pub struct Https {
    agent: ureq::Agent,
    token: Token,
}

/// How many times a call Discord rate-limits is tried, and the longest wait between tries.
const TRIES: u32 = 4;
const LONGEST_WAIT: f64 = 30.0;

impl Https {
    pub fn new(token: Token) -> Self {
        let agent = ureq::Agent::config_builder()
            // Discord asks bots to name themselves this way.
            .user_agent(concat!(
                "DiscordBot (https://github.com/dereth-network/dereth, ",
                env!("CARGO_PKG_VERSION"),
                ")"
            ))
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .into();
        Self { agent, token }
    }

    /// One call: its JSON answer (`null` for an empty one), or the error Discord gave.
    fn call(&mut self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let url = format!("{API}{path}");
        let auth = format!("Bot {}", self.token.0);
        let what = format!("{method} {path}");
        for _ in 0..TRIES {
            let result = match method {
                "GET" => self.agent.get(&url).header("Authorization", &auth).call(),
                "DELETE" => self
                    .agent
                    .delete(&url)
                    .header("Authorization", &auth)
                    .call(),
                "PUT" => self
                    .agent
                    .put(&url)
                    .header("Authorization", &auth)
                    .send(&[][..]),
                _ => {
                    let bytes = body.map(Value::to_string).unwrap_or_default();
                    self.agent
                        .post(&url)
                        .header("Authorization", &auth)
                        .content_type("application/json")
                        .send(bytes.as_bytes())
                }
            };
            let mut response = result.map_err(|e| format!("{what}: {e}"))?;
            let status = response.status().as_u16();
            let text = response
                .body_mut()
                .with_config()
                .limit(1 << 24)
                .read_to_string()
                .map_err(|e| format!("{what}: reading the answer: {e}"))?;
            let json: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            match status {
                429 => {
                    let wait = json["retry_after"].as_f64().unwrap_or(1.0);
                    std::thread::sleep(Duration::from_secs_f64(wait.clamp(0.0, LONGEST_WAIT)));
                }
                200..=299 => return Ok(json),
                _ => {
                    let message = json["message"].as_str().unwrap_or("no message");
                    return Err(format!("{what}: Discord answered {status}: {message}"));
                }
            }
        }
        Err(format!("{what}: still rate-limited after {TRIES} tries"))
    }
}

/// A string field of a JSON object, or the error naming it.
fn field(v: &Value, key: &str, what: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{what} without `{key}`"))
}

impl Discord for Https {
    fn me(&mut self) -> Result<String, String> {
        let v = self.call("GET", "/users/@me", None)?;
        field(&v, "id", "the bot's user")
    }

    fn guild(&mut self, channel: &str) -> Result<Option<String>, String> {
        let v = self.call("GET", &format!("/channels/{channel}"), None)?;
        Ok(v["guild_id"].as_str().map(str::to_owned))
    }

    fn pins(&mut self, channel: &str) -> Result<Vec<Pinned>, String> {
        // The first page holds the 50 most recently pinned, more than a channel of change logs keeps.
        let v = self.call("GET", &format!("/channels/{channel}/messages/pins"), None)?;
        let items = v["items"].as_array().ok_or("the pins without `items`")?;
        items
            .iter()
            .map(|item| {
                let m = &item["message"];
                Ok(Pinned {
                    id: field(m, "id", "a pinned message")?,
                    author: field(&m["author"], "id", "a pinned message's author")?,
                    content: m["content"].as_str().unwrap_or_default().to_owned(),
                })
            })
            .collect()
    }

    fn post(&mut self, channel: &str, content: &str) -> Result<String, String> {
        // No mentions: a change log pings nobody, whatever it says.
        let body = json!({ "content": content, "allowed_mentions": { "parse": [] } });
        let v = self.call(
            "POST",
            &format!("/channels/{channel}/messages"),
            Some(&body),
        )?;
        field(&v, "id", "the new message")
    }

    fn pin(&mut self, channel: &str, message: &str) -> Result<(), String> {
        self.call(
            "PUT",
            &format!("/channels/{channel}/messages/pins/{message}"),
            None,
        )
        .map(drop)
    }

    fn unpin(&mut self, channel: &str, message: &str) -> Result<(), String> {
        self.call(
            "DELETE",
            &format!("/channels/{channel}/messages/pins/{message}"),
            None,
        )
        .map(drop)
    }
}

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub product: Product,
    pub version: String,
    pub release_url: Option<String>,
    pub dry_run: bool,
}

const USAGE: &str = "usage: cargo xtask announce <product> <version> | --tag <tag> \
                     [--release-url <url>] [--dry-run]";

/// The request, `Ok(None)` for a tag that is none of [`PRODUCTS`]' (announced as nothing).
///
/// # Errors
/// The usage, or what is wrong with the version.
pub fn parse(args: &[String]) -> Result<Option<Request>, String> {
    let mut positional = Vec::new();
    let mut tag = None;
    let mut release_url = None;
    let mut dry_run = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| USAGE.to_owned());
        match a.as_str() {
            "--tag" => tag = Some(value()?),
            "--release-url" => release_url = Some(value()?).filter(|u| !u.trim().is_empty()),
            "--dry-run" => dry_run = true,
            other if other.starts_with("--") => {
                return Err(format!("unexpected argument `{other}`\n{USAGE}"))
            }
            other => positional.push(other.to_owned()),
        }
    }
    let (product, version) = match (tag, positional.as_slice()) {
        (Some(t), []) => match product_of_tag(&t) {
            Some((p, v)) => (p, v.to_owned()),
            None => return Ok(None),
        },
        (None, [p, v]) => (
            PRODUCTS
                .into_iter()
                .find(|x| x.command_name() == p)
                .ok_or_else(|| format!("`{p}` keeps no change log to announce\n{USAGE}"))?,
            v.clone(),
        ),
        _ => return Err(USAGE.to_owned()),
    };
    version::parse_release_version(&version)?;
    Ok(Some(Request {
        product,
        version,
        release_url,
        dry_run,
    }))
}

/// The release's highlights: its section of the product's `CHANGES.md` as its tag has it, or as
/// the working tree has it when there is no such tag here.
fn highlights(ws: &Path, product: Product, version: &str) -> Option<String> {
    let file = notes::changes_file(product);
    let tag = product.tag_for(version);
    let changes = git(ws, &["show", &format!("{tag}:{file}")])
        .or_else(|_| std::fs::read_to_string(ws.join(file)).map_err(|e| e.to_string()))
        .unwrap_or_default();
    notes::section(&changes, version)
}

/// A warning, on the console and, in a workflow, as an annotation and in the run's summary.
fn warn(message: &str) {
    eprintln!("announce: warning: {message}");
    if std::env::var_os("GITHUB_ACTIONS").is_some() {
        println!("::warning title=Discord::{message}");
    }
    if let Some(summary) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(summary) {
            let _ = writeln!(f, "Not announced on Discord: {message}");
        }
    }
}

/// `cargo xtask announce ...`.
pub fn announce_command(args: &[String]) -> i32 {
    let request = match parse(args) {
        Ok(Some(r)) => r,
        Ok(None) => {
            println!("announce: that tag's product keeps no change log; nothing to announce");
            return 0;
        }
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let Request {
        product,
        version,
        release_url,
        dry_run,
    } = request;
    if version.contains('-') {
        println!("announce: {version} is a pre-release; only final releases are announced");
        return 0;
    }
    let ws = crate::util::workspace_root();
    let highlights = highlights(&ws, product, &version);
    let release_url = release_url.unwrap_or_else(|| {
        format!(
            "{}/releases/tag/{}",
            package::DEFAULT_SOURCE_URL,
            product.tag_for(&version)
        )
    });
    if dry_run {
        let previous = "(the link to the post pinned before it)";
        let text = post_text(
            product,
            &version,
            highlights.as_deref(),
            &release_url,
            Some(previous),
        );
        for (i, part) in split(&text, MESSAGE_LIMIT).iter().enumerate() {
            if i > 0 {
                println!("--- (the next message)");
            }
            println!("{part}");
        }
        return 0;
    }
    let var = channel_var(product);
    let Some(channel) = std::env::var(&var).ok().filter(|c| !c.trim().is_empty()) else {
        println!(
            "announce: {var} is not set; {} is not announced",
            product.display_name()
        );
        return 0;
    };
    let Some(token) = std::env::var(TOKEN_VAR)
        .ok()
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
    else {
        warn(&format!("{TOKEN_VAR} is not set"));
        return 0;
    };
    let mut discord = Https::new(Token(token));
    match announce(
        &mut discord,
        channel.trim(),
        product,
        &version,
        highlights.as_deref(),
        &release_url,
    ) {
        Ok(Outcome::Posted { id, unpinned }) => {
            println!(
                "announce: posted {} {version} as message {id} and pinned it; unpinned {}",
                product.display_name(),
                if unpinned.is_empty() {
                    "none".to_owned()
                } else {
                    unpinned.join(", ")
                }
            );
        }
        Ok(Outcome::AlreadyAnnounced {
            id,
            version: pinned,
        }) => {
            println!(
                "announce: message {id}, pinned, already announces {} {pinned}; nothing posted",
                product.display_name()
            );
        }
        Err(e) => warn(&e),
    }
    0
}

#[cfg(test)]
mod tests;
