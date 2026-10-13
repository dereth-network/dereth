//! The Discord broker: what the hub does with the Discord application's bot token, which it alone
//! holds (`docs/network/05-hub.md` section 5.6). Worlds hold no Discord secret and run no Discord
//! code; they speak to the hub as they always have.
//!
//! **[`Rest`]** is the small client for the parts of Discord's HTTP API the broker uses: the
//! application and its commands, a server's channels and their webhooks ([`crate::bridge`]), the
//! bot's own direct messages with a person and their lines ([`crate::dms`]), and the answers to the
//! bot's `/dn` command ([`crate::interactions`]). It honors a `429` for
//! the route it was given on (or, for a global one, every route), and an answer saying its route
//! has no call left until a moment: until then, a call on that route asks Discord nothing and
//! answers [`BrokerError::Wait`]. Its base address is a setting, so tests and development use a
//! stand-in.
//!
//! Nothing here writes the bot token or a webhook's token to the log, and the [`Debug`] of each
//! shows none of them.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::discord::{any_address, wait_of};
use crate::fetch::{self, Fetch, Url};

/// Discord's API.
pub const API: &str = crate::discord::API;

/// How long the hub waits before trying again for a webhook it could not find or make.
pub const RETRY: Duration = Duration::from_secs(60);

/// How often the hub looks again for each mirror's webhook.
pub const RECHECK: Duration = Duration::from_secs(3_600);

/// The most of an answer from Discord the broker reads.
const MAX_ANSWER: usize = 256 * 1024;

/// The broker's settings: the bot token, Discord's address, and how often it asks Discord again.
#[derive(Clone)]
pub struct Broker {
    /// Read from the host's environment or a file the hub's operator names; never logged.
    pub bot_token: String,
    /// Discord's API ([`API`]; a stand-in in tests and development).
    pub api: String,
    /// Which addresses the broker may connect to: any, for Discord itself.
    pub rule: fetch::AddressRule,
    /// How long it waits before trying again for a webhook it could not find or make ([`RETRY`]).
    pub retry: Duration,
    /// How often it looks again for each mirror's webhook ([`RECHECK`]).
    pub recheck: Duration,
    /// The address the requests' `User-Agent` names, as Discord asks a bot's to.
    pub site: String,
    /// How often it reads a person's direct messages while a reply window is open
    /// ([`crate::dms::POLL`]).
    pub poll: Duration,
    /// Each person's pace of tells into its Discord ([`crate::dms::DMS_PER_PERSON`]).
    pub dm_pace: dereth_network_protocol::limits::Rate,
    /// How long a person may answer a tell that went to its Discord
    /// ([`crate::dms::REPLY_WINDOW`]).
    pub reply_window: Duration,
    /// How long no tell is tried in a person's direct messages after Discord refused one
    /// ([`crate::dms::REFUSED_FOR`]).
    pub refused_for: Duration,
}

impl Broker {
    /// The broker with `bot_token` and Discord's own address.
    #[must_use]
    pub fn new(bot_token: String) -> Self {
        Self {
            bot_token,
            api: API.into(),
            rule: any_address,
            retry: RETRY,
            recheck: RECHECK,
            site: "https://dereth.network".into(),
            poll: crate::dms::POLL,
            dm_pace: crate::dms::DMS_PER_PERSON,
            reply_window: crate::dms::REPLY_WINDOW,
            refused_for: crate::dms::REFUSED_FOR,
        }
    }
}

impl std::fmt::Debug for Broker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Broker")
            .field("api", &self.api)
            .field("poll", &self.poll)
            .finish_non_exhaustive()
    }
}

/// Why the broker did not do what it was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokerError {
    /// Discord asked the hub to wait on this route this many more seconds; it was not asked.
    Wait(u64),
    /// Discord has no such channel (or member, line or webhook).
    NotFound,
    /// Discord refused the bot: it may not see or do this (`403`).
    NoAccess,
    /// Discord refused the name a webhook's line was to show.
    NameRefused,
    /// Anything else: Discord refusing, an answer that is not one, Discord unreachable, the
    /// registry. Never holds a token.
    Failed(String),
}

impl std::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wait(secs) => write!(f, "Discord asks for nothing more here for {secs} seconds"),
            Self::NotFound => f.write_str("Discord has no such thing"),
            Self::NoAccess => f.write_str("Discord does not let the bot do this"),
            Self::NameRefused => f.write_str("Discord refused the name the line was to show"),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

impl From<crate::registry::RegistryError> for BrokerError {
    fn from(e: crate::registry::RegistryError) -> Self {
        Self::Failed(e.message)
    }
}

/// Who a call is made as.
#[derive(Debug, Clone, Copy)]
pub enum Auth {
    /// The application's bot.
    Bot,
    /// No one: a webhook's call, whose token is in its path.
    None,
}

/// The waits Discord's `429`s asked for: by route, and for every route.
#[derive(Debug, Default)]
struct Waits {
    routes: HashMap<String, Instant>,
    all: Option<Instant>,
}

impl Waits {
    /// How long `route` must still wait at `now`; none when it need not.
    fn left_exactly(&self, route: &str, now: Instant) -> Option<Duration> {
        let until = [self.routes.get(route).copied(), self.all]
            .into_iter()
            .flatten()
            .max()?;
        until.checked_duration_since(now).filter(|d| !d.is_zero())
    }

    /// The whole seconds `route` must still wait at `now`; none when it need not.
    fn left(&self, route: &str, now: Instant) -> Option<u64> {
        let left = self.left_exactly(route, now)?;
        Some(left.as_secs() + u64::from(left.subsec_nanos() > 0))
    }

    /// Discord asked for nothing on `route` (every route, when `global`) for `secs` from `now`.
    fn wait(&mut self, route: &str, secs: u64, global: bool, now: Instant) {
        self.wait_for(route, Duration::from_secs(secs), global, now);
    }

    /// Nothing on `route` (every route, when `global`) for `left` from `now`.
    fn wait_for(&mut self, route: &str, left: Duration, global: bool, now: Instant) {
        let until = now + left;
        if self.routes.len() > 10_000 {
            self.routes.retain(|_, t| *t > now);
        }
        if global {
            self.all = Some(self.all.map_or(until, |t| t.max(until)));
        } else {
            self.routes.insert(route.to_owned(), until);
        }
    }
}

/// How long an answer says its route has no call left: its `X-RateLimit-Reset-After` when its
/// `X-RateLimit-Remaining` is 0, at most [`crate::discord::MAX_WAIT_SECS`]; none otherwise.
fn spent_for(headers: &[(String, String)]) -> Option<Duration> {
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim())
    };
    if header("x-ratelimit-remaining")? != "0" {
        return None;
    }
    let secs: f64 = header("x-ratelimit-reset-after")?.parse().ok()?;
    #[allow(clippy::cast_precision_loss)]
    let max = crate::discord::MAX_WAIT_SECS as f64;
    (secs.is_finite() && secs > 0.0).then(|| Duration::from_secs_f64(secs.min(max)))
}

/// Whether a `429` is for every route: its `X-RateLimit-Global` or `X-RateLimit-Scope`, or its
/// body's `global`.
fn is_global(headers: &[(String, String)], body: &[u8]) -> bool {
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim().to_ascii_lowercase())
    };
    header("x-ratelimit-global").is_some_and(|v| v == "true")
        || header("x-ratelimit-scope").is_some_and(|v| v == "global")
        || serde_json::from_slice::<Value>(body).is_ok_and(|v| v["global"] == true)
}

/// `id` when it is a Discord id (a snowflake: 1 to 24 digits), so that it can go in a path.
fn snowflake(id: &str) -> Result<&str, BrokerError> {
    if is_snowflake(id) {
        Ok(id)
    } else {
        Err(BrokerError::Failed("not a Discord id".into()))
    }
}

/// Whether `id` is a Discord id: 1 to 24 digits.
#[must_use]
pub fn is_snowflake(id: &str) -> bool {
    (1..=24).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

/// Whether `token` may be a webhook's token: Discord's are URL-safe, and go in a path.
fn is_webhook_token(token: &str) -> bool {
    (1..=256).contains(&token.len())
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The client for Discord's HTTP API.
pub struct Rest {
    broker: Broker,
    waits: Mutex<Waits>,
}

impl std::fmt::Debug for Rest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rest")
            .field("broker", &self.broker)
            .finish_non_exhaustive()
    }
}

impl Rest {
    /// A client with `broker`'s settings.
    #[must_use]
    pub fn new(broker: Broker) -> Self {
        Self {
            broker,
            waits: Mutex::new(Waits::default()),
        }
    }

    /// The broker's settings.
    #[must_use]
    pub fn broker(&self) -> &Broker {
        &self.broker
    }

    fn waits(&self) -> std::sync::MutexGuard<'_, Waits> {
        self.waits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// One call: `route` names it for Discord's waits (its method and path, never a token). The
    /// status and the body read as JSON (`null` when empty).
    fn call(
        &self,
        route: &str,
        method: &str,
        path: &str,
        auth: Auth,
        body: Option<&Value>,
    ) -> Result<(u16, Value), BrokerError> {
        if let Some(secs) = self.waits().left(route, Instant::now()) {
            return Err(BrokerError::Wait(secs));
        }
        let url = Url::parse(&format!("{}{path}", self.broker.api.trim_end_matches('/')))
            .map_err(BrokerError::Failed)?;
        let mut headers = vec![
            (
                "User-Agent".to_owned(),
                format!(
                    "DiscordBot ({}, {})",
                    self.broker.site,
                    env!("CARGO_PKG_VERSION")
                ),
            ),
            ("Accept".to_owned(), "application/json".to_owned()),
        ];
        if let Auth::Bot = auth {
            headers.push((
                "Authorization".to_owned(),
                format!("Bot {}", self.broker.bot_token),
            ));
        }
        let body = body.map(Value::to_string).unwrap_or_default().into_bytes();
        if !body.is_empty() {
            headers.push(("Content-Type".to_owned(), "application/json".to_owned()));
        }
        let r = Fetch {
            method,
            url,
            headers,
            body,
            timeout: Duration::from_secs(10),
            max_body: MAX_ANSWER,
            max_redirects: 0,
            rule: self.broker.rule,
            deny: Vec::new(),
            check: fetch::any_url,
            check_with: Vec::new(),
        }
        .send()
        .map_err(|e| BrokerError::Failed(format!("Discord: {e}")))?;
        if r.status == 429 {
            let secs = wait_of(&r.headers, &r.body);
            let global = is_global(&r.headers, &r.body);
            self.waits().wait(route, secs, global, Instant::now());
            crate::server::log(
                "info",
                &format!(
                    "Discord asked the broker to wait {secs} seconds{}",
                    if global { " on every route" } else { "" }
                ),
            );
            return Err(BrokerError::Wait(secs));
        }
        // A route with no call left waits until it has one, rather than being refused.
        if let Some(left) = spent_for(&r.headers) {
            self.waits().wait_for(route, left, false, Instant::now());
        }
        let json = if r.body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&r.body).unwrap_or(Value::Null)
        };
        match r.status {
            200..=299 => Ok((r.status, json)),
            403 => Err(BrokerError::NoAccess),
            404 => Err(BrokerError::NotFound),
            400 if json["errors"]["username"].is_object() => Err(BrokerError::NameRefused),
            s => Err(BrokerError::Failed(format!("Discord answered {s}"))),
        }
    }

    /// How long a call on `route` must still wait before it asks Discord anything; none when it
    /// need not.
    #[must_use]
    pub fn waiting(&self, route: &str) -> Option<Duration> {
        self.waits().left_exactly(route, Instant::now())
    }

    /// The application the bot token is for, as Discord describes it: its id, its bot user's id,
    /// and the public key Discord signs its interactions with.
    ///
    /// # Errors
    /// Any.
    pub fn application(&self) -> Result<Application, BrokerError> {
        let (_, app) = self.call(
            "GET /applications/@me",
            "GET",
            "/applications/@me",
            Auth::Bot,
            None,
        )?;
        let id = |v: &Value| {
            v.as_str()
                .and_then(|i| snowflake(i).ok())
                .map(str::to_owned)
                .ok_or_else(|| BrokerError::Failed("Discord gave no application".into()))
        };
        Ok(Application {
            id: id(&app["id"])?,
            bot_id: id(&app["bot"]["id"])?,
            verify_key: app["verify_key"]
                .as_str()
                .filter(|k| crate::interactions::public_key(k).is_some())
                .map(str::to_owned),
        })
    }

    /// Replaces the application `app`'s commands with `commands`, as the bot: the same commands
    /// again change nothing.
    ///
    /// # Errors
    /// Any.
    pub fn register_commands(&self, app: &str, commands: &Value) -> Result<(), BrokerError> {
        let path = format!("/applications/{}/commands", snowflake(app)?);
        self.call(
            &format!("PUT {path}"),
            "PUT",
            &path,
            Auth::Bot,
            Some(commands),
        )
        .map(|_| ())
    }

    /// Edits the answer the hub gave an interaction of the application `app` to say `content`, with
    /// no menu left on it, calling nobody whatever it holds: the interaction's `token` is in the
    /// path, and on no log.
    ///
    /// # Errors
    /// Any; [`BrokerError::NotFound`] once the token has lapsed.
    pub fn edit_original(&self, app: &str, token: &str, content: &str) -> Result<(), BrokerError> {
        if !crate::interactions::is_interaction_token(token) {
            return Err(BrokerError::NotFound);
        }
        let path = format!("/webhooks/{}/{token}/messages/@original", snowflake(app)?);
        let body = json!({
            "content": content, "components": [], "allowed_mentions": { "parse": [] },
        });
        self.call(
            &format!("PATCH /webhooks/{app}/messages/@original"),
            "PATCH",
            &path,
            Auth::None,
            Some(&body),
        )
        .map(|_| ())
    }

    /// The channel `id`, as Discord describes it to the bot.
    ///
    /// # Errors
    /// Any; [`BrokerError::NotFound`] or [`BrokerError::NoAccess`] for one the bot cannot see.
    pub fn channel(&self, id: &str) -> Result<Value, BrokerError> {
        let path = format!("/channels/{}", snowflake(id)?);
        self.call(&format!("GET {path}"), "GET", &path, Auth::Bot, None)
            .map(|(_, v)| v)
    }

    /// The server `id`, with its roles, as Discord describes it to the bot.
    ///
    /// # Errors
    /// Any.
    pub fn guild(&self, id: &str) -> Result<Value, BrokerError> {
        let path = format!("/guilds/{}", snowflake(id)?);
        self.call(&format!("GET {path}"), "GET", &path, Auth::Bot, None)
            .map(|(_, v)| v)
    }

    /// `user` as a member of the server `guild` (its nickname and roles); none when it is not one.
    ///
    /// # Errors
    /// Any but a missing member.
    pub fn guild_member(&self, guild: &str, user: &str) -> Result<Option<Value>, BrokerError> {
        let path = format!("/guilds/{}/members/{}", snowflake(guild)?, snowflake(user)?);
        match self.call(
            &format!("GET /guilds/{guild}/members"),
            "GET",
            &path,
            Auth::Bot,
            None,
        ) {
            Ok((_, v)) => Ok(Some(v)),
            Err(BrokerError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The webhooks of `channel`.
    ///
    /// # Errors
    /// Any; [`BrokerError::NoAccess`] when the bot may not manage them.
    pub fn webhooks(&self, channel: &str) -> Result<Vec<Value>, BrokerError> {
        let path = format!("/channels/{}/webhooks", snowflake(channel)?);
        let (_, hooks) = self.call(&format!("GET {path}"), "GET", &path, Auth::Bot, None)?;
        Ok(hooks.as_array().cloned().unwrap_or_default())
    }

    /// Makes a webhook named `name` in `channel`.
    ///
    /// # Errors
    /// Any.
    pub fn create_webhook(&self, channel: &str, name: &str) -> Result<Webhook, BrokerError> {
        let path = format!("/channels/{}/webhooks", snowflake(channel)?);
        let (_, made) = self.call(
            &format!("POST {path}"),
            "POST",
            &path,
            Auth::Bot,
            Some(&json!({ "name": name })),
        )?;
        Webhook::of(&made).ok_or_else(|| BrokerError::Failed("Discord gave no webhook".into()))
    }

    /// The webhook `id`, read with its own token as anyone holding its address may (the bot is not
    /// asked): the webhook, and the channel it says lines in.
    ///
    /// # Errors
    /// Any; [`BrokerError::NotFound`] for a webhook Discord does not have under that token.
    pub fn webhook(&self, id: &str, token: &str) -> Result<(Webhook, String), BrokerError> {
        if !is_webhook_token(token) {
            return Err(BrokerError::NotFound);
        }
        let path = format!("/webhooks/{}/{token}", snowflake(id)?);
        let (_, hook) = self.call(
            &format!("GET /webhooks/{id}"),
            "GET",
            &path,
            Auth::None,
            None,
        )?;
        let gone = || BrokerError::NotFound;
        if hook["id"].as_str() != Some(id) || hook["type"] != 1 {
            return Err(gone());
        }
        let guild = hook["guild_id"].as_str().filter(|g| is_snowflake(g));
        let channel = hook["channel_id"].as_str().filter(|c| is_snowflake(c));
        let (Some(guild), Some(channel)) = (guild, channel) else {
            return Err(gone());
        };
        Ok((
            Webhook {
                id: id.to_owned(),
                guild: guild.to_owned(),
                token: token.to_owned(),
            },
            channel.to_owned(),
        ))
    }

    /// Says `content` through `hook`, shown as `username` (the webhook's own name when none) with
    /// the picture at `avatar_url`, calling nobody whatever it holds and showing no link's
    /// preview: the line's id.
    ///
    /// # Errors
    /// Any; [`BrokerError::NameRefused`] when Discord refuses `username`, and
    /// [`BrokerError::NotFound`] once the webhook is gone.
    pub fn execute_webhook(
        &self,
        hook: &Webhook,
        content: &str,
        username: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<String, BrokerError> {
        let path = format!(
            "/webhooks/{}/{}?wait=true",
            snowflake(&hook.id)?,
            hook.token
        );
        let mut body = json!({
            "content": content,
            "allowed_mentions": { "parse": [] },
            "flags": SUPPRESS_EMBEDS,
        });
        if let Some(name) = username {
            body["username"] = json!(name);
        }
        if let Some(url) = avatar_url {
            body["avatar_url"] = json!(url);
        }
        let (_, sent) = self.call(
            &webhook_route(&hook.id),
            "POST",
            &path,
            Auth::None,
            Some(&body),
        )?;
        sent["id"]
            .as_str()
            .and_then(|i| snowflake(i).ok())
            .map(str::to_owned)
            .ok_or_else(|| BrokerError::Failed("Discord gave no line id".into()))
    }

    /// Opens the bot's direct messages with the Discord user `user`, or finds them open: their
    /// channel's id. Discord lets the bot open them only with a user who shares a server with it.
    ///
    /// # Errors
    /// Any; [`BrokerError::NoAccess`] when Discord refuses.
    pub fn open_dm(&self, user: &str) -> Result<String, BrokerError> {
        let body = json!({ "recipient_id": snowflake(user)? });
        let (_, channel) = self.call(
            OPEN_DM_ROUTE,
            "POST",
            "/users/@me/channels",
            Auth::Bot,
            Some(&body),
        )?;
        channel["id"]
            .as_str()
            .and_then(|i| snowflake(i).ok())
            .map(str::to_owned)
            .ok_or_else(|| BrokerError::Failed("Discord gave no channel".into()))
    }

    /// Says `content` as the bot in the direct messages `channel`, calling nobody whatever it holds
    /// and showing no link's preview: the line's id.
    ///
    /// # Errors
    /// Any; [`BrokerError::NoAccess`] when Discord refuses (the user lets no member of a server
    /// it shares with the bot message it, or shares none), [`BrokerError::NotFound`] once Discord
    /// no longer has the channel.
    pub fn send_dm(&self, channel: &str, content: &str) -> Result<String, BrokerError> {
        let path = format!("/channels/{}/messages", snowflake(channel)?);
        let body = json!({
            "content": content,
            "allowed_mentions": { "parse": [] },
            "flags": SUPPRESS_EMBEDS,
        });
        let (_, sent) = self.call(&dm_route(channel), "POST", &path, Auth::Bot, Some(&body))?;
        sent["id"]
            .as_str()
            .and_then(|i| snowflake(i).ok())
            .map(str::to_owned)
            .ok_or_else(|| BrokerError::Failed("Discord gave no line id".into()))
    }

    /// The lines of the direct messages `channel` after the line `after` (the oldest `limit` of
    /// them, 1 to 100), or without `after` its newest `limit`, in any order.
    ///
    /// # Errors
    /// Any.
    pub fn channel_lines(
        &self,
        channel: &str,
        after: Option<u64>,
        limit: u32,
    ) -> Result<Vec<Value>, BrokerError> {
        let mut path = format!(
            "/channels/{}/messages?limit={}",
            snowflake(channel)?,
            limit.clamp(1, 100)
        );
        if let Some(after) = after {
            path.push_str(&format!("&after={after}"));
        }
        let (_, lines) = self.call(&channel_lines_route(channel), "GET", &path, Auth::Bot, None)?;
        Ok(lines.as_array().cloned().unwrap_or_default())
    }
}

/// A line said through a webhook shows no preview of the links it holds (Discord's
/// `SUPPRESS_EMBEDS`).
pub const SUPPRESS_EMBEDS: u64 = 1 << 2;

/// The route a webhook's lines are said on, for Discord's waits.
#[must_use]
pub fn webhook_route(hook: &str) -> String {
    format!("POST /webhooks/{hook}")
}

/// The route a direct messages channel's lines are read on, for Discord's waits.
#[must_use]
pub fn channel_lines_route(channel: &str) -> String {
    format!("GET /channels/{channel}/messages")
}

/// The route the bot's direct messages are opened on, for Discord's waits.
pub const OPEN_DM_ROUTE: &str = "POST /users/@me/channels";

/// The route the bot's lines are said on in the direct messages `channel`, for Discord's waits.
#[must_use]
pub fn dm_route(channel: &str) -> String {
    format!("POST /channels/{channel}/messages")
}

/// The Discord application the bot token is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Application {
    pub id: String,
    /// Its bot user's id.
    pub bot_id: String,
    /// The public key Discord signs the application's interactions with, in hex; none when Discord
    /// gave none that reads as one.
    pub verify_key: Option<String>,
}

/// A channel's webhook, with the token to say lines through it.
#[derive(Clone, PartialEq, Eq)]
pub struct Webhook {
    pub id: String,
    /// The server its channel is in.
    pub guild: String,
    token: String,
}

impl Webhook {
    /// The webhook Discord describes in `v`, when it holds its id, its server and its token.
    #[must_use]
    pub fn of(v: &Value) -> Option<Self> {
        let id = v["id"].as_str().filter(|i| is_snowflake(i))?;
        let guild = v["guild_id"].as_str().filter(|i| is_snowflake(i))?;
        let token = v["token"].as_str().filter(|t| is_webhook_token(t))?;
        Some(Self {
            id: id.to_owned(),
            guild: guild.to_owned(),
            token: token.to_owned(),
        })
    }

    /// A webhook the registry keeps: its id, its server and its token, when each reads as one.
    #[must_use]
    pub fn kept(id: &str, guild: &str, token: &str) -> Option<Self> {
        (is_snowflake(id) && is_snowflake(guild) && is_webhook_token(token)).then(|| Self {
            id: id.to_owned(),
            guild: guild.to_owned(),
            token: token.to_owned(),
        })
    }

    /// Its token, for the registry to keep; never logged, never answered.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl std::fmt::Debug for Webhook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Webhook")
            .field("id", &self.id)
            .field("guild", &self.guild)
            .finish_non_exhaustive()
    }
}

/// The id and the token of a webhook's address as Discord gives it to whoever makes one
/// (`https://discord.com/api/webhooks/<id>/<token>`, on `discord.com`, `discordapp.com`, or their
/// `ptb.` and `canary.` hosts, the API's version optional); none for any other address. The hub
/// asks Discord's own API about it, never the host the address names.
#[must_use]
pub fn webhook_address(url: &str) -> Option<(String, String)> {
    let rest = url.trim().strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let host = host.to_ascii_lowercase();
    let hosts = ["discord.com", "discordapp.com"];
    let known = hosts
        .iter()
        .any(|h| host == *h || host == format!("ptb.{h}") || host == format!("canary.{h}"));
    if !known {
        return None;
    }
    let path = path.strip_prefix("api/")?;
    let path = match path.split_once('/') {
        Some((v, rest))
            if v.len() > 1 && v.starts_with('v') && v[1..].bytes().all(|b| b.is_ascii_digit()) =>
        {
            rest
        }
        _ => path,
    };
    let mut parts = path.strip_prefix("webhooks/")?.split('/');
    let (id, token) = (parts.next()?, parts.next()?);
    (parts.next().is_none() && is_snowflake(id) && is_webhook_token(token))
        .then(|| (id.to_owned(), token.to_owned()))
}

/// What the broker keeps while the hub runs: the client, what the mirrors need
/// ([`crate::bridge`]), what the tells into persons' Discord need ([`crate::dms`]), and what the
/// bot's `/dn` command needs ([`crate::interactions`]).
#[derive(Debug)]
pub(crate) struct Service {
    pub(crate) rest: Rest,
    pub(crate) bridge: crate::bridge::State,
    pub(crate) tells: crate::dms::State,
    pub(crate) speak: crate::interactions::State,
}

impl Service {
    pub(crate) fn new(broker: Broker) -> Self {
        Self {
            rest: Rest::new(broker),
            bridge: crate::bridge::State::new(),
            tells: crate::dms::State::new(),
            speak: crate::interactions::State::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: hub: the broker's settings never show the bot token, nor does its client, nor a
    /// webhook its token.
    #[test]
    fn the_broker_shows_no_token() {
        let b = Broker::new("bot-token-value".into());
        assert!(!format!("{b:?}").contains("bot-token-value"));
        assert!(!format!("{:?}", Rest::new(b)).contains("bot-token-value"));
        let hook = Webhook::kept("1558376954619629608", "1558376951000000000", "hook-token_x")
            .expect("a webhook");
        assert!(!format!("{hook:?}").contains("hook-token_x"));
        assert_eq!(hook.token(), "hook-token_x");
    }

    /// Spec: hub: a `429` makes the route it came on wait, and a global one every route, for the
    /// whole seconds it names; another route goes on.
    #[test]
    fn a_429_waits_its_route_or_every_route() {
        let now = Instant::now();
        let mut w = Waits::default();
        assert_eq!(w.left("POST /webhooks/1", now), None);
        w.wait("POST /webhooks/1", 30, false, now);
        assert_eq!(w.left("POST /webhooks/1", now), Some(30));
        assert_eq!(
            w.left("POST /webhooks/1", now + Duration::from_millis(29_500)),
            Some(1)
        );
        assert_eq!(
            w.left("POST /webhooks/1", now + Duration::from_secs(30)),
            None
        );
        assert_eq!(
            w.left("GET /channels/1", now),
            None,
            "another route goes on"
        );
        w.wait("GET /channels/1", 5, true, now);
        assert_eq!(w.left("GET /applications/@me", now), Some(5), "every route");
        assert_eq!(w.left("POST /webhooks/1", now), Some(30));
        let h = |k: &str, v: &str| vec![(k.to_owned(), v.to_owned())];
        assert!(is_global(&h("X-RateLimit-Global", "true"), b""));
        assert!(is_global(&h("x-ratelimit-scope", "global"), b""));
        assert!(is_global(&[], br#"{"retry_after": 2, "global": true}"#));
        assert!(!is_global(
            &h("X-RateLimit-Scope", "user"),
            br#"{"global": false}"#
        ));
    }

    /// Spec: hub: a webhook's address is read for its id and its token only on Discord's own
    /// hosts, in the form Discord gives it; any other address is none.
    #[test]
    fn a_webhooks_address_is_discords_own_form() {
        let id_token = |id: &str, t: &str| Some((id.to_owned(), t.to_owned()));
        assert_eq!(
            webhook_address("https://discord.com/api/webhooks/1558376954619629608/abc-DEF_123"),
            id_token("1558376954619629608", "abc-DEF_123")
        );
        assert_eq!(
            webhook_address("https://canary.discordapp.com/api/v10/webhooks/155/tok"),
            id_token("155", "tok")
        );
        assert_eq!(
            webhook_address("https://PTB.Discord.com/api/webhooks/155/tok"),
            id_token("155", "tok")
        );
        for bad in [
            "http://discord.com/api/webhooks/155/tok",
            "https://discord.com.evil.test/api/webhooks/155/tok",
            "https://evil.test/api/webhooks/155/tok",
            "https://discord.com/api/webhooks/155",
            "https://discord.com/api/webhooks/155/tok/slack",
            "https://discord.com/api/webhooks/1x5/tok",
            "https://discord.com/api/webhooks/155/to%2Fk",
            "https://discord.com/api/webhooks/155/tok?wait=true",
            "https://discord.com/webhooks/155/tok",
            "",
        ] {
            assert_eq!(webhook_address(bad), None, "{bad}");
        }
    }
}
