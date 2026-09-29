//! The server the page connects to: a WebSocket URL, and the rule for which ones may be used.
//!
//! The client connects to a server's WebSocket endpoint directly (`wss://`), or to
//! `dereth-web-relay` on this machine (`ws://127.0.0.1:<port>/`) for a server that speaks only UDP;
//! the frame is the same either way. The game protocol is not encrypted (its key stream only keys
//! the checksums, and the login carries the account's password), so a plain `ws://` URL is refused
//! for anything but the loopback address.

/// The client's own numbering of the server's two ports: the frame's ports are relative (the first
/// datagram's is the logon port), so every server is addressed at this one.
pub const NOMINAL_SERVER: &str = "127.0.0.1:9000";

/// Why `url` cannot be the server, or `None` when it can: `wss://` to any host, `ws://` only to the
/// loopback address.
#[must_use]
pub fn problem(url: &str) -> Option<String> {
    let url = url.trim();
    let (secure, rest) = if let Some(rest) = url.strip_prefix("wss://") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("ws://") {
        (false, rest)
    } else {
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
    }
}
