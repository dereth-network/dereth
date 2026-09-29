//! Where releases are found: the GitHub releases of the repository this build names as its source
//! (`empyrean_common::brand::SOURCE_URL`, set at build time), read through GitHub's REST API over
//! HTTPS.

use std::time::Duration;

use serde::Deserialize;

use super::version::Version;

/// The largest download accepted (an archive is about 20 MB).
pub const DOWNLOAD_LIMIT: u64 = 512 * 1024 * 1024;

/// A repository's releases, and how to reach them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The API address of the repository (`https://api.github.com/repos/<owner>/<name>`).
    pub api: String,
    /// `<owner>/<name>`.
    pub repo: String,
}

impl Source {
    /// The source for a GitHub repository address (`https://github.com/<owner>/<name>`).
    ///
    /// # Errors
    /// When `url` is not a GitHub repository address.
    pub fn github(url: &str) -> Result<Self, String> {
        let rest = url
            .trim()
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .strip_prefix("https://github.com/")
            .ok_or_else(|| {
                format!("{url} is not a GitHub repository, so its releases cannot be looked up")
            })?;
        let mut parts = rest.split('/');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(owner), Some(name), None) if !owner.is_empty() && !name.is_empty() => Ok(Self {
                api: format!("https://api.github.com/repos/{owner}/{name}"),
                repo: format!("{owner}/{name}"),
            }),
            _ => Err(format!("{url} is not a GitHub repository address")),
        }
    }

    /// This build's source.
    ///
    /// # Errors
    /// As [`Source::github`].
    pub fn this_build() -> Result<Self, String> {
        Self::github(empyrean_common::brand::SOURCE_URL)
    }

    /// The source `server.update_source` names: a GitHub repository address, or the address of a
    /// mirror serving GitHub's releases API for a repository (`http(s)://<host>/.../repos/<owner>/<name>`);
    /// empty means [`Source::this_build`].
    ///
    /// # Errors
    /// When `configured` is neither.
    pub fn configured(configured: &str) -> Result<Self, String> {
        let url = configured.trim().trim_end_matches('/');
        if url.is_empty() {
            return Self::this_build();
        }
        if url.starts_with("https://github.com/") {
            return Self::github(url);
        }
        let mirror = url.starts_with("https://") || url.starts_with("http://");
        let mut tail = url.rsplit('/');
        match (tail.next(), tail.next(), tail.next()) {
            (Some(name), Some(owner), Some("repos"))
                if mirror && !name.is_empty() && !owner.is_empty() =>
            {
                Ok(Self {
                    api: url.to_owned(),
                    repo: format!("{owner}/{name}"),
                })
            }
            _ => Err(format!(
                "server.update_source = \"{configured}\" is neither a GitHub repository (https://github.com/<owner>/<name>) nor a releases API address (https://<host>/repos/<owner>/<name>)"
            )),
        }
    }
}

/// A release as the releases API lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub version: Version,
    pub prerelease: bool,
    /// Asset name and download address.
    pub assets: Vec<(String, String)>,
}

impl Published {
    /// The download address of the asset named `name`.
    #[must_use]
    pub fn asset(&self, name: &str) -> Option<&str> {
        self.assets
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, u)| u.as_str())
    }
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// Reads a releases API answer: the published Empyrean releases (drafts and other tags left out).
///
/// # Errors
/// When `json` is not such an answer.
pub fn parse_releases(json: &str) -> Result<Vec<Published>, String> {
    let list: Vec<ApiRelease> =
        serde_json::from_str(json).map_err(|e| format!("the release list cannot be read: {e}"))?;
    Ok(list
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            Some(Published {
                version: Version::from_tag(&r.tag_name)?,
                prerelease: r.prerelease,
                assets: r
                    .assets
                    .into_iter()
                    .map(|a| (a.name, a.browser_download_url))
                    .collect(),
            })
        })
        .collect())
}

/// The HTTP client the updater uses.
#[must_use]
pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!("empyrean-server/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .into()
}

fn http_error(url: &str, e: &ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(c @ (403 | 429)) => format!(
            "{url} refused the request (HTTP {c}), usually GitHub's hourly limit on unauthenticated API calls: the next check tries again"
        ),
        ureq::Error::StatusCode(c) => format!("{url} answered HTTP {c}"),
        other => format!("could not reach {url}: {other}"),
    }
}

/// The published releases of `source`, newest first as the API lists them.
///
/// # Errors
/// When the API cannot be reached or answers something else.
pub fn list(agent: &ureq::Agent, source: &Source) -> Result<Vec<Published>, String> {
    let url = format!("{}/releases?per_page=100", source.api);
    let text = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .and_then(|mut r| r.body_mut().read_to_string())
        .map_err(|e| http_error(&url, &e))?;
    parse_releases(&text)
}

/// Downloads `url` into memory (at most [`DOWNLOAD_LIMIT`] bytes).
///
/// # Errors
/// When it cannot be downloaded.
pub fn download(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    agent
        .get(url)
        .call()
        .and_then(|mut r| {
            r.body_mut()
                .with_config()
                .limit(DOWNLOAD_LIMIT)
                .read_to_vec()
        })
        .map_err(|e| http_error(url, &e))
}
