//! Where Empyrean's releases are found: the shared reading of a repository's releases
//! ([`dereth_server_update::source`]), of the repository this build names as its source
//! (`empyrean_common::brand::SOURCE_URL`, set at build time) and of the tags beginning
//! [`TAG_PREFIX`].

use super::version::{Version, TAG_PREFIX};

pub use dereth_server_update::source::{
    download, next_page, Page, Published, Source, DOWNLOAD_LIMIT, MAX_PAGES,
};

/// This build's source.
///
/// # Errors
/// When the build's repository is not a GitHub repository.
pub fn this_build() -> Result<Source, String> {
    Source::github(empyrean_common::brand::SOURCE_URL)
}

/// The source `server.update_source` names; empty means [`this_build`].
///
/// # Errors
/// When `configured` is neither a GitHub repository nor a mirror of its releases API.
pub fn configured(configured: &str) -> Result<Source, String> {
    Source::configured(configured, empyrean_common::brand::SOURCE_URL)
}

/// Reads a releases API answer: the published Empyrean releases (drafts and other tags left out).
///
/// # Errors
/// When `json` is not such an answer.
pub fn parse_releases(json: &str) -> Result<Vec<Published>, String> {
    dereth_server_update::source::parse_releases(TAG_PREFIX, json)
}

/// The HTTP client the updater uses.
#[must_use]
pub fn agent() -> ureq::Agent {
    dereth_server_update::source::agent(concat!("empyrean-server/", env!("CARGO_PKG_VERSION")))
}

/// The published Empyrean releases of `source`, as
/// [`dereth_server_update::source::list`] reads them.
///
/// # Errors
/// When the API cannot be reached or answers something else.
pub fn list(
    agent: &ureq::Agent,
    source: &Source,
    current: &Version,
) -> Result<Vec<Published>, String> {
    dereth_server_update::source::list(agent, source, TAG_PREFIX, current)
}

/// [`list`] over `fetch`, which answers the page of the list at an address.
///
/// # Errors
/// When a page cannot be fetched or read.
pub fn list_pages(
    source: &Source,
    current: &Version,
    fetch: &mut dyn FnMut(&str) -> Result<Page, String>,
) -> Result<Vec<Published>, String> {
    dereth_server_update::source::list_pages(source, TAG_PREFIX, current, fetch)
}
