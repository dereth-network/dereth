//! The GitHub REST API calls `publish` makes, over a [`Transport`]: the real one is HTTPS with
//! `ureq` ([`Https`]); the tests put an in-memory GitHub behind the same trait.
//!
//! A [`GitHub`] made for a dry run refuses every call that would change anything before it is
//! sent, so a dry run cannot write even through a mistake in the steps above it.

use std::time::Duration;

use serde_json::Value;

/// The variables the token is read from, in order.
pub const TOKEN_VARS: &[&str] = &["DERETH_PUBLISH_TOKEN", "GITHUB_TOKEN"];

/// Where the REST API is.
pub const API: &str = "https://api.github.com";

/// The REST API version every call asks for.
const API_VERSION: &str = "2022-11-28";

/// An HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Patch,
    Delete,
}

impl Method {
    pub fn name(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }

    /// Whether the call changes anything on GitHub.
    pub fn writes(self) -> bool {
        self != Self::Get
    }
}

/// One call.
#[derive(Debug)]
pub struct HttpRequest<'a> {
    pub method: Method,
    pub url: String,
    pub accept: &'static str,
    pub content_type: Option<&'static str>,
    pub body: Option<&'a [u8]>,
}

/// Its answer: the status and the body, whatever the status.
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Something that sends calls to GitHub.
pub trait Transport {
    /// Send `request`; an error only when no answer came.
    fn send(&mut self, request: &HttpRequest<'_>) -> Result<HttpResponse, String>;
}

/// The token. It is sent in the `Authorization` header and nowhere else: it is never printed,
/// logged, written or put in a message, and `Debug` does not show it.
pub struct Token(pub(super) String);

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(hidden)")
    }
}

impl Token {
    /// The token in the first of [`TOKEN_VARS`] that is set, and the variable's name.
    pub fn from_env() -> Option<(Self, &'static str)> {
        TOKEN_VARS.iter().find_map(|var| {
            std::env::var(var)
                .ok()
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
                .map(|v| (Self(v), *var))
        })
    }
}

/// The real transport: HTTPS with rustls and the bundled web roots, no system TLS library.
#[derive(Debug)]
pub struct Https {
    agent: ureq::Agent,
    token: Token,
}

impl Https {
    pub fn new(token: Token) -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(concat!("dereth-xtask-publish/", env!("CARGO_PKG_VERSION")))
            // Every status is an answer: the steps say what a 404 or a 422 means.
            .http_status_as_error(false)
            // A download is answered with a redirect to another host, which must not get the
            // token (ureq's default, said here because it matters).
            .redirect_auth_headers(ureq::config::RedirectAuthHeaders::Never)
            // The largest release file is a few hundred megabytes.
            .timeout_global(Some(Duration::from_secs(3600)))
            .build()
            .into();
        Self { agent, token }
    }
}

/// The largest answer read: more than the largest release file.
const BODY_LIMIT: u64 = 1 << 30;

impl Transport for Https {
    fn send(&mut self, r: &HttpRequest<'_>) -> Result<HttpResponse, String> {
        let auth = format!("Bearer {}", self.token.0);
        let what = || format!("{} {}", r.method.name(), r.url);
        let result = match r.method {
            Method::Get | Method::Delete => {
                let b = if r.method == Method::Get {
                    self.agent.get(&r.url)
                } else {
                    self.agent.delete(&r.url)
                };
                b.header("Authorization", &auth)
                    .header("Accept", r.accept)
                    .header("X-GitHub-Api-Version", API_VERSION)
                    .call()
            }
            Method::Post | Method::Patch => {
                let b = if r.method == Method::Post {
                    self.agent.post(&r.url)
                } else {
                    self.agent.patch(&r.url)
                };
                b.header("Authorization", &auth)
                    .header("Accept", r.accept)
                    .header("X-GitHub-Api-Version", API_VERSION)
                    .content_type(r.content_type.unwrap_or("application/json"))
                    .send(r.body.unwrap_or_default())
            }
        };
        let mut response = result.map_err(|e| {
            format!(
                "{}: {e}. Check the network connection (a proxy is read from HTTPS_PROXY or \
                 ALL_PROXY)",
                what()
            )
        })?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(BODY_LIMIT)
            .read_to_vec()
            .map_err(|e| format!("{}: reading the answer: {e}", what()))?;
        Ok(HttpResponse { status, body })
    }
}

/// A release, as the API describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub id: u64,
    pub tag_name: String,
    pub name: String,
    pub body: String,
    pub draft: bool,
    pub prerelease: bool,
    pub upload_url: String,
    pub html_url: String,
}

impl Release {
    fn from_json(v: &Value) -> Result<Self, String> {
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
        Ok(Self {
            id: v["id"].as_u64().ok_or("a release without an id")?,
            tag_name: s("tag_name"),
            name: s("name"),
            body: s("body"),
            draft: v["draft"].as_bool().unwrap_or(false),
            prerelease: v["prerelease"].as_bool().unwrap_or(false),
            upload_url: s("upload_url"),
            html_url: s("html_url"),
        })
    }
}

/// A file of a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteAsset {
    pub id: u64,
    pub name: String,
    pub label: String,
    pub size: u64,
    /// `uploaded`, or `starter` for an upload that never finished.
    pub state: String,
    /// The SHA-256 GitHub computed, when it says (`sha256:<hex>`).
    pub digest: Option<String>,
}

impl RemoteAsset {
    fn from_json(v: &Value) -> Result<Self, String> {
        Ok(Self {
            id: v["id"].as_u64().ok_or("an asset without an id")?,
            name: v["name"].as_str().unwrap_or_default().to_owned(),
            label: v["label"].as_str().unwrap_or_default().to_owned(),
            size: v["size"].as_u64().unwrap_or(0),
            state: v["state"].as_str().unwrap_or("uploaded").to_owned(),
            digest: v["digest"]
                .as_str()
                .and_then(|d| d.strip_prefix("sha256:"))
                .map(str::to_ascii_lowercase),
        })
    }

    /// Whether the upload finished.
    pub fn complete(&self) -> bool {
        self.state == "uploaded"
    }
}

/// `text` as a URL query value: every byte but the unreserved ones percent-encoded.
pub fn query_value(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// One repository's releases, through a transport.
pub struct GitHub<'t> {
    transport: &'t mut dyn Transport,
    /// `owner/name`.
    pub repo: String,
    api: String,
    writes: bool,
}

impl std::fmt::Debug for GitHub<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHub")
            .field("repo", &self.repo)
            .field("writes", &self.writes)
            .finish_non_exhaustive()
    }
}

const JSON: &str = "application/vnd.github+json";

impl<'t> GitHub<'t> {
    /// `writes`: whether calls that change anything may be sent at all.
    pub fn new(transport: &'t mut dyn Transport, repo: &str, writes: bool) -> Self {
        Self {
            transport,
            repo: repo.to_owned(),
            api: API.to_owned(),
            writes,
        }
    }

    fn call(
        &mut self,
        method: Method,
        url: String,
        accept: &'static str,
        body: Option<(&'static str, &[u8])>,
    ) -> Result<HttpResponse, String> {
        if method.writes() && !self.writes {
            return Err(format!(
                "a dry run sends nothing that changes the release ({} {url} was not sent)",
                method.name()
            ));
        }
        self.transport.send(&HttpRequest {
            method,
            url,
            accept,
            content_type: body.map(|(t, _)| t),
            body: body.map(|(_, b)| b),
        })
    }

    fn repo_url(&self, path: &str) -> String {
        format!("{}/repos/{}{path}", self.api, self.repo)
    }

    /// The JSON of a call answered with one of `ok`, or why not.
    fn json(
        &mut self,
        method: Method,
        url: String,
        body: Option<&Value>,
        ok: &[u16],
    ) -> Result<Value, String> {
        let bytes = body.map(|b| serde_json::to_vec(b).unwrap_or_default());
        let shown = format!("{} {url}", method.name());
        let response = self.call(
            method,
            url,
            JSON,
            bytes.as_deref().map(|b| ("application/json", b)),
        )?;
        if !ok.contains(&response.status) {
            return Err(refusal(&shown, &response));
        }
        if response.body.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&response.body).map_err(|e| format!("{shown}: the answer: {e}"))
    }

    /// That the token can read the repository, so a missing tag below is a missing tag.
    pub fn check_access(&mut self) -> Result<(), String> {
        self.json(Method::Get, self.repo_url(""), None, &[200])
            .map(|_| ())
    }

    /// The commit tag `tag` names on GitHub, or `None` when GitHub has no such tag.
    pub fn tag_commit(&mut self, tag: &str) -> Result<Option<String>, String> {
        let url = self.repo_url(&format!("/git/ref/tags/{}", query_value(tag)));
        let shown = format!("GET {url}");
        let response = self.call(Method::Get, url, JSON, None)?;
        if response.status == 404 {
            return Ok(None);
        }
        if response.status != 200 {
            return Err(refusal(&shown, &response));
        }
        let mut object: Value =
            serde_json::from_slice(&response.body).map_err(|e| format!("{shown}: {e}"))?;
        // An annotated tag names a tag object, which names the commit.
        for _ in 0..4 {
            let kind = object["object"]["type"].as_str().unwrap_or_default();
            let sha = object["object"]["sha"]
                .as_str()
                .ok_or_else(|| format!("{shown}: the answer names no object"))?
                .to_owned();
            match kind {
                "commit" => return Ok(Some(sha)),
                "tag" => {
                    object = self.json(
                        Method::Get,
                        self.repo_url(&format!("/git/tags/{sha}")),
                        None,
                        &[200],
                    )?;
                }
                other => return Err(format!("{tag} names a {other}, not a commit")),
            }
        }
        Err(format!("{tag} is a tag of a tag of a tag: too deep"))
    }

    /// Whether `commit` is on `branch` (the branch holds it).
    pub fn on_branch(&mut self, commit: &str, branch: &str) -> Result<bool, String> {
        let v = self.json(
            Method::Get,
            self.repo_url(&format!("/compare/{commit}...{branch}")),
            None,
            &[200],
        )?;
        Ok(matches!(v["status"].as_str(), Some("ahead" | "identical")))
    }

    /// Every release of the repository, drafts among them.
    pub fn releases(&mut self) -> Result<Vec<Release>, String> {
        let mut out = Vec::new();
        for page in 1.. {
            let v = self.json(
                Method::Get,
                self.repo_url(&format!("/releases?per_page=100&page={page}")),
                None,
                &[200],
            )?;
            let items = v.as_array().ok_or("the release list is not a list")?;
            for item in items {
                out.push(Release::from_json(item)?);
            }
            if items.len() < 100 {
                break;
            }
        }
        Ok(out)
    }

    /// Every file of release `id`.
    pub fn assets(&mut self, id: u64) -> Result<Vec<RemoteAsset>, String> {
        let mut out = Vec::new();
        for page in 1.. {
            let v = self.json(
                Method::Get,
                self.repo_url(&format!("/releases/{id}/assets?per_page=100&page={page}")),
                None,
                &[200],
            )?;
            let items = v.as_array().ok_or("the asset list is not a list")?;
            for item in items {
                out.push(RemoteAsset::from_json(item)?);
            }
            if items.len() < 100 {
                break;
            }
        }
        Ok(out)
    }

    /// The bytes of a release file.
    pub fn download(&mut self, asset: &RemoteAsset) -> Result<Vec<u8>, String> {
        let url = self.repo_url(&format!("/releases/assets/{}", asset.id));
        let shown = format!("GET {url} ({})", asset.name);
        let response = self.call(Method::Get, url, "application/octet-stream", None)?;
        if response.status != 200 {
            return Err(refusal(&shown, &response));
        }
        Ok(response.body)
    }

    /// Create a release from `fields`.
    pub fn create_release(&mut self, fields: &Value) -> Result<Release, String> {
        let v = self.json(
            Method::Post,
            self.repo_url("/releases"),
            Some(fields),
            &[201],
        )?;
        Release::from_json(&v)
    }

    /// Change release `id`'s `fields`.
    pub fn update_release(&mut self, id: u64, fields: &Value) -> Result<Release, String> {
        let v = self.json(
            Method::Patch,
            self.repo_url(&format!("/releases/{id}")),
            Some(fields),
            &[200],
        )?;
        Release::from_json(&v)
    }

    /// Add a file to `release`, shown as `label`.
    pub fn upload(
        &mut self,
        release: &Release,
        name: &str,
        label: &str,
        bytes: &[u8],
    ) -> Result<RemoteAsset, String> {
        // `upload_url` is a URI template: `.../assets{?name,label}`.
        let base = release
            .upload_url
            .split('{')
            .next()
            .unwrap_or(&release.upload_url);
        let url = format!(
            "{base}?name={}&label={}",
            query_value(name),
            query_value(label)
        );
        let shown = format!("POST {base} ({name})");
        let response = self.call(
            Method::Post,
            url,
            JSON,
            Some(("application/octet-stream", bytes)),
        )?;
        if response.status != 201 {
            return Err(refusal(&shown, &response));
        }
        let v: Value =
            serde_json::from_slice(&response.body).map_err(|e| format!("{shown}: {e}"))?;
        RemoteAsset::from_json(&v)
    }

    /// Remove a file from its release.
    pub fn delete_asset(&mut self, asset: &RemoteAsset) -> Result<(), String> {
        self.json(
            Method::Delete,
            self.repo_url(&format!("/releases/assets/{}", asset.id)),
            None,
            &[204],
        )
        .map(|_| ())
    }
}

/// A call GitHub answered with a status the step did not want, said for someone who has to act.
fn refusal(shown: &str, response: &HttpResponse) -> String {
    let message = serde_json::from_slice::<Value>(&response.body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_owned))
        .unwrap_or_default();
    let hint = match response.status {
        401 => ": the token was refused (expired, or mistyped)",
        403 | 404 => {
            ": the token cannot see or change this repository's releases (a fine-grained token \
             for this repository with Contents: read and write)"
        }
        422 => ": GitHub refused the request as it was made",
        _ => "",
    };
    format!(
        "{shown} answered HTTP {}{hint}{}",
        response.status,
        if message.is_empty() {
            String::new()
        } else {
            format!(" ({message})")
        }
    )
}
