//! A plugin's own HTTPS requests, to the domains its manifest lists.
//!
//! `net.fetch{ url = …, method = "GET", body = …, headers = { … }, as = "text" | "image" }` answers
//! a promise of `{ status, body, type }`, or with `as = "image"` of an image name `ui.image` can
//! show. Only `https://` to a host the manifest's `[net] domains` admits, with the `net.http`
//! permission, at most [`IN_FLIGHT`] at a time per plugin. The transfer is the product's
//! ([`PluginNet`]): a blocking client off the frame's thread on the desktop, the browser's fetch on
//! the web; the answer arrives as a settlement on a later frame.

use dereth_plugin_api::{CallError, Value};

/// The most requests one plugin may have outstanding.
pub const IN_FLIGHT: usize = 4;
/// The most bytes an answer's body may hold.
pub const MAX_BODY: usize = 8 << 20;

/// A request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

/// An answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// A product's HTTPS transfers.
pub trait PluginNet: std::fmt::Debug {
    /// Begin request `id`.
    fn start(&mut self, id: i64, request: HttpRequest);
    /// The requests that ended since the last call: each id with its answer or why it failed.
    fn poll(&mut self) -> Vec<(i64, Result<HttpResponse, String>)>;
}

/// No network: every request fails.
#[derive(Debug, Default)]
pub struct NoNet {
    ended: Vec<i64>,
}

impl PluginNet for NoNet {
    fn start(&mut self, id: i64, _request: HttpRequest) {
        self.ended.push(id);
    }

    fn poll(&mut self) -> Vec<(i64, Result<HttpResponse, String>)> {
        self.ended
            .drain(..)
            .map(|id| (id, Err("this host has no network for plugins".to_owned())))
            .collect()
    }
}

/// Answers from a table, by URL: for tests and for hosts that serve fixed files.
#[derive(Debug, Default, Clone)]
pub struct TableNet {
    pub answers: std::collections::BTreeMap<String, HttpResponse>,
    ended: Vec<(i64, String)>,
    /// Every request made, in order.
    pub asked: Vec<HttpRequest>,
}

impl PluginNet for TableNet {
    fn start(&mut self, id: i64, request: HttpRequest) {
        self.ended.push((id, request.url.clone()));
        self.asked.push(request);
    }

    fn poll(&mut self) -> Vec<(i64, Result<HttpResponse, String>)> {
        let answers = &self.answers;
        self.ended
            .drain(..)
            .map(|(id, url)| {
                (
                    id,
                    answers
                        .get(&url)
                        .cloned()
                        .ok_or_else(|| "not found".to_owned()),
                )
            })
            .collect()
    }
}

/// The host and port of an `https://` URL: what comes before the first `/`, `\`, `?` or `#`
/// after the scheme, the port 443 when it names none. `None` for any other scheme, a URL with
/// credentials, a port that is not one, or a host that is an address rather than a name.
#[must_use]
pub fn https_authority(url: &str) -> Option<(&str, u16)> {
    let rest = url.strip_prefix("https://")?;
    let end = rest.find(['/', '\\', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.contains('@') {
        return None;
    }
    dereth_plugin_api::manifest::net_domain(authority)
}

/// Whether a request may reach `ip`: an address on the public internet, not the machine itself
/// (loopback), not a private or shared network, not a link-local or unspecified one, and not a
/// multicast, broadcast or documentation one. A host name that resolves to anything else is not
/// reached.
#[must_use]
pub fn is_public_address(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(a) => {
            let [b0, b1, b2, _] = a.octets();
            !(a.is_loopback()
                || a.is_private()
                || a.is_link_local()
                || a.is_unspecified()
                || a.is_multicast()
                || a.is_broadcast()
                || a.is_documentation()
                // 0.0.0.0/8, 100.64.0.0/10 (shared address space), 192.0.0.0/24, 198.18.0.0/15
                // and 240.0.0.0/4.
                || b0 == 0
                || (b0 == 100 && (64..128).contains(&b1))
                || (b0 == 192 && b1 == 0 && b2 == 0)
                || (b0 == 198 && (18..20).contains(&b1))
                || b0 >= 240)
        }
        IpAddr::V6(a) => {
            if let Some(v4) = a.to_ipv4_mapped() {
                return is_public_address(IpAddr::V4(v4));
            }
            let s = a.segments();
            !(a.is_loopback()
                || a.is_unspecified()
                || a.is_multicast()
                // fc00::/7 (unique local), fe80::/10 (link-local), 2001:db8::/32 (documentation).
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || (s[0] == 0x2001 && s[1] == 0x0db8))
        }
    }
}

/// Read a request from a plugin's table.
///
/// # Errors
/// No URL, a method other than GET, POST, PUT or DELETE, or headers that are not strings.
pub fn request_of(v: &Value) -> Result<(HttpRequest, bool), CallError> {
    let bad = |m: &str| CallError::BadArgs(m.to_owned());
    for k in v.as_map().into_iter().flatten().map(|(k, _)| k) {
        if !["url", "method", "headers", "body", "as"].contains(&k.as_str()) {
            return Err(CallError::BadArgs(format!("net.fetch has no field {k:?}")));
        }
    }
    let url = v
        .get("url")
        .as_str()
        .ok_or_else(|| bad("net.fetch needs a url"))?
        .to_owned();
    let method = v
        .get("method")
        .as_str()
        .unwrap_or("GET")
        .to_ascii_uppercase();
    if !["GET", "POST", "PUT", "DELETE"].contains(&method.as_str()) {
        return Err(bad("net.fetch's method is GET, POST, PUT or DELETE"));
    }
    let mut headers = Vec::new();
    for (k, h) in v.get("headers").as_map().into_iter().flatten() {
        let h = h
            .as_str()
            .ok_or_else(|| bad("net.fetch's headers are strings"))?;
        headers.push((k.clone(), h.to_owned()));
    }
    let image = match v.get("as").as_str() {
        None | Some("text") => false,
        Some("image") => true,
        Some(_) => return Err(bad("net.fetch's as is \"text\" or \"image\"")),
    };
    Ok((
        HttpRequest {
            method,
            url,
            headers,
            body: v.get("body").as_str().map(str::to_owned),
        },
        image,
    ))
}

#[cfg(test)]
mod tests {
    use super::{https_authority, is_public_address};

    #[test]
    fn only_https_urls_to_a_name_have_a_host_and_credentials_are_refused() {
        assert_eq!(
            https_authority("https://maps.example.org/a.png?x=1"),
            Some(("maps.example.org", 443))
        );
        assert_eq!(
            https_authority("https://example.org:8443"),
            Some(("example.org", 8443))
        );
        // A backslash ends the host, as a browser reads it.
        assert_eq!(
            https_authority("https://evil.org\\@example.org/"),
            Some(("evil.org", 443))
        );
        for refused in [
            "http://example.org/",
            "https://user@example.org/",
            "https:///x",
            "https://127.0.0.1/",
            "https://[::1]/",
            "https://0x7f.1/",
            "https://example.org:0/",
            "https://example.org:/",
        ] {
            assert_eq!(https_authority(refused), None, "{refused}");
        }
    }

    #[test]
    fn only_addresses_on_the_public_internet_are_reached() {
        for private in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "0.0.0.0",
            "100.64.0.1",
            "255.255.255.255",
            "::1",
            "::",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
        ] {
            assert!(
                !is_public_address(private.parse().unwrap()),
                "{private} is not public"
            );
        }
        for public in ["93.184.216.34", "1.1.1.1", "2606:4700:4700::1111"] {
            assert!(is_public_address(public.parse().unwrap()), "{public}");
        }
    }
}
