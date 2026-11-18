//! The launcher's requests for the place packages of worlds' eras ([`dereth_launch::places`]): the
//! release lists and the world databases they name, over HTTPS.

use std::time::Duration;

use dereth_places::Fetch;

/// The launcher's HTTP client for the place packages.
pub struct Http(ureq::Agent);

impl Http {
    pub fn new() -> Self {
        Self(
            ureq::Agent::config_builder()
                .https_only(true)
                .user_agent(concat!("dereth-launcher/", env!("CARGO_PKG_VERSION")))
                // A world database is tens of megabytes: a slow download is not cut short, but
                // nothing waits forever, and a join waits for it the first time only.
                .timeout_connect(Some(Duration::from_secs(20)))
                .timeout_recv_response(Some(Duration::from_secs(60)))
                .timeout_global(Some(Duration::from_secs(10 * 60)))
                .build()
                .into(),
        )
    }
}

fn plain(url: &str, e: &ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(c) => format!("{url} answered HTTP {c}"),
        other => format!("could not reach {url}: {other}"),
    }
}

impl Fetch for Http {
    fn text(&self, url: &str, accept: &str) -> Result<String, String> {
        self.0
            .get(url)
            .header("Accept", accept)
            .call()
            .and_then(|mut r| r.body_mut().read_to_string())
            .map_err(|e| plain(url, &e))
    }

    fn bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        self.0
            .get(url)
            .call()
            .and_then(|mut r| r.body_mut().with_config().limit(limit).read_to_vec())
            .map_err(|e| plain(url, &e))
    }
}
