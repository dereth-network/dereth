//! How a browser reaches a world itself: the world's WebSocket endpoint, which a web client
//! connects to in place of the game's UDP, and the world's status document, which names it.
//!
//! The game protocol is not encrypted (its key stream only keys the checksums, and the login
//! carries the account's password), so a launcher follows a `wss://` URL to any host, and a plain
//! `ws://` one only to this machine (`dereth-web-relay`, or a world on the player's own machine).
//! The rule is one for every address a launcher is given: the one a player types, a status
//! document's, and the one the hub's list names, so no list can point a launcher anywhere else.
//! A status document is `https://` likewise, or `http://` on this machine.
//!
//! Two addresses name one world when they differ only in what a URL leaves to its reader: the
//! case of the scheme and the host, the scheme's own port, and a path that is only a `/`. A status
//! address with no path names the world's `/status`, as the page asks it.

/// Why `url` cannot be the WebSocket a client connects to, or `None` when it can: `wss://` to any
/// host, `ws://` only to this machine.
#[must_use]
pub fn problem(url: &str) -> Option<String> {
    let url = url.trim();
    let Some((secure, rest)) = split_scheme(url, "wss://", "ws://") else {
        return Some(format!(
            "{url}: the server is a wss:// URL, or ws://127.0.0.1:<port>/ for dereth-web-relay"
        ));
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return Some(format!("{url}: the URL names no server"));
    }
    if secure || is_loopback(host(authority)) {
        None
    } else {
        Some(format!(
            "{url}: plain ws:// is refused for a server on another machine, because the game \
             protocol is not encrypted and the login carries the password; use wss://"
        ))
    }
}

/// Whether a client may connect to `url` ([`problem`]).
#[must_use]
pub fn is_websocket_url(url: &str) -> bool {
    problem(url).is_none()
}

/// Whether `url` may be a world's status document: `https://` to any host, `http://` only to this
/// machine.
#[must_use]
pub fn is_status_url(url: &str) -> bool {
    let url = url.trim();
    let Some((secure, rest)) = split_scheme(url, "https://", "http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    !authority.is_empty() && !authority.contains('@') && (secure || is_loopback(host(authority)))
}

/// Whether `a` and `b` are one WebSocket endpoint.
#[must_use]
pub fn same_websocket(a: &str, b: &str) -> bool {
    same(a, b, "/")
}

/// Whether `a` and `b` are one status document.
#[must_use]
pub fn same_status(a: &str, b: &str) -> bool {
    same(a, b, "/status")
}

/// Whether `a` and `b` are one address, a bare path read as `bare`.
fn same(a: &str, b: &str, bare: &str) -> bool {
    match (canonical(a, bare), canonical(b, bare)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// `url` as the one spelling it compares by: the scheme and the host in lower case, no port where
/// it is the scheme's own, `bare` for no path or `/`, no `/` at the path's end. `None` for no URL.
fn canonical(url: &str, bare: &str) -> Option<String> {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.is_empty() {
        return None;
    }
    let mut authority = authority.to_ascii_lowercase();
    let own = match scheme.as_str() {
        "wss" | "https" => ":443",
        "ws" | "http" => ":80",
        _ => "",
    };
    if !own.is_empty() && authority.ends_with(own) {
        authority.truncate(authority.len() - own.len());
    }
    let tail = match tail.trim_end_matches('/') {
        "" => bare,
        t => t,
    };
    Some(format!("{scheme}://{authority}{tail}"))
}

/// Whether `url` starts with `secure` or `plain` (without regard to case), and what follows.
fn split_scheme<'a>(url: &'a str, secure: &str, plain: &str) -> Option<(bool, &'a str)> {
    let starts = |s: &str| {
        url.get(..s.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(s))
    };
    if starts(secure) {
        Some((true, &url[secure.len()..]))
    } else if starts(plain) {
        Some((false, &url[plain.len()..]))
    } else {
        None
    }
}

/// The host of an authority, without its port: `[::1]` for `[::1]:9180`.
fn host(authority: &str) -> &str {
    if authority.starts_with('[') {
        return authority
            .split_once(']')
            .map_or(authority, |(h, _)| &authority[..=h.len()]);
    }
    authority.rsplit_once(':').map_or(authority, |(h, _)| h)
}

fn is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") || host == "[::1]" {
        return true;
    }
    host.parse::<std::net::Ipv4Addr>()
        .is_ok_and(|ip| ip.is_loopback())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's rule for the addresses it connects to).
    use super::*;

    #[test]
    fn wss_is_accepted_to_any_server_and_ws_only_to_this_machine() {
        assert_eq!(problem("wss://play.example.org/ws"), None);
        assert_eq!(problem("wss://play.example.org:9443/"), None);
        assert_eq!(problem("ws://127.0.0.1:9180/"), None);
        assert_eq!(problem("ws://localhost:9180"), None);
        assert_eq!(problem("ws://[::1]:9180/"), None);
        assert_eq!(problem("ws://127.0.0.2:9180/"), None);
        assert!(problem("ws://play.example.org/ws").is_some());
        assert!(problem("ws://192.168.1.5:9180/").is_some());
        assert!(problem("ws://127.0.0.1.example.org/").is_some());
        assert!(problem("ws://127.0.0.1@example.org/").is_some());
    }

    #[test]
    fn anything_but_a_websocket_url_is_refused() {
        assert!(problem("127.0.0.1:9000").is_some());
        assert!(problem("https://play.example.org").is_some());
        assert!(problem("wss://").is_some());
        assert!(!is_websocket_url("javascript:alert(1)"));
    }

    #[test]
    fn a_status_document_is_https_or_plain_only_to_this_machine() {
        assert!(is_status_url("https://play.example.org/status"));
        assert!(is_status_url("http://127.0.0.1:9100/status"));
        assert!(!is_status_url("http://play.example.org/status"));
        assert!(!is_status_url("wss://play.example.org/status"));
        assert!(!is_status_url("https://user@play.example.org/status"));
    }

    #[test]
    fn two_spellings_of_one_address_are_one_world() {
        assert!(same_status(
            "https://Play.Example.org:443/",
            "https://play.example.org/status"
        ));
        assert!(same_status(
            "https://play.example.org/status/",
            "HTTPS://play.example.org/status"
        ));
        assert!(same_websocket(
            "wss://play.example.org",
            "wss://play.example.org:443/"
        ));
        assert!(!same_websocket(
            "wss://play.example.org/ws",
            "wss://play.example.org/"
        ));
        assert!(!same_status(
            "https://a.example.org/status",
            "https://b.example.org/status"
        ));
        assert!(!same_websocket(
            "ws://127.0.0.1:9180/",
            "ws://127.0.0.1:9181/"
        ));
        assert!(!same_status("", ""));
    }
}
