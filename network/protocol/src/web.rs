//! A world's web addresses: where a browser reaches it without the game's UDP.
//!
//! A world names them in its `hello` ([`crate::Message::Hello`]): `websocket_url`, its WebSocket
//! endpoint, which a web client connects to in place of the game's UDP, and `status_url`, its
//! status document. The hub keeps what the latest `hello` named and lists it while the world's
//! link is up, so a launcher reaches the world directly.
//!
//! The game protocol is not encrypted (its key stream only keys the checksums, and the login
//! carries the account's password), so a web address is a secure URL to any host (`wss://`,
//! `https://`), or a plain one only to this machine (`ws://`, `http://` to `127.0.0.0/8`, `[::1]`
//! or `localhost`), for a world and a player on one machine. Each is at most [`MAX_URL_BYTES`] of
//! printable ASCII without a quote, an angle bracket, a backslash or a backtick, and names a host
//! (letters, digits, dots and hyphens, or an IPv6 address in brackets) with no user in it, and an
//! optional port. A URL that breaks the rule is not kept.

/// The longest web address, in bytes.
pub const MAX_URL_BYTES: usize = 200;

/// Whether `url` may be a world's WebSocket endpoint: `wss://` to any host, `ws://` only to this
/// machine.
#[must_use]
pub fn is_websocket_url(url: &str) -> bool {
    allowed(url, "wss://", "ws://")
}

/// Whether `url` may be a world's status document: `https://` to any host, `http://` only to this
/// machine.
#[must_use]
pub fn is_status_url(url: &str) -> bool {
    allowed(url, "https://", "http://")
}

/// Whether `url` is a `secure` URL to any host, or a `plain` one to this machine, within the rule.
fn allowed(url: &str, secure: &str, plain: &str) -> bool {
    if url.is_empty()
        || url.len() > MAX_URL_BYTES
        || !url.bytes().all(|b| {
            b.is_ascii_graphic() && !matches!(b, b'<' | b'>' | b'"' | b'\'' | b'\\' | b'`')
        })
    {
        return false;
    }
    let scheme = |s: &str| {
        url.get(..s.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(s))
    };
    let (is_secure, rest) = if scheme(secure) {
        (true, &url[secure.len()..])
    } else if scheme(plain) {
        (false, &url[plain.len()..])
    } else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let Some((host, port)) = split_authority(authority) else {
        return false;
    };
    let port_ok = port.is_none_or(|p| {
        !p.is_empty()
            && p.len() <= 5
            && p.bytes().all(|b| b.is_ascii_digit())
            && p.parse::<u16>().is_ok_and(|n| n > 0)
    });
    port_ok && (is_secure || is_loopback(host))
}

/// The host and the port of an authority (`play.example.org:9443`, `[::1]:9180`); `None` when the
/// host is not a host name or a bracketed IPv6 address.
fn split_authority(authority: &str) -> Option<(&str, Option<&str>)> {
    let (host, port) = if authority.starts_with('[') {
        let end = authority.find(']')?;
        let (host, after) = authority.split_at(end + 1);
        let inner = &host[1..host.len() - 1];
        if inner.is_empty() || inner.parse::<std::net::Ipv6Addr>().is_err() {
            return None;
        }
        match after {
            "" => (host, None),
            _ => (host, Some(after.strip_prefix(':')?)),
        }
    } else {
        let (host, port) = match authority.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        };
        if host.is_empty()
            || !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        {
            return None;
        }
        (host, port)
    };
    Some((host, port))
}

/// Whether `host` is this machine: `localhost`, an IPv4 loopback address, or `[::1]`.
fn is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return inner
            .parse::<std::net::Ipv6Addr>()
            .is_ok_and(|ip| ip.is_loopback());
    }
    host.parse::<std::net::Ipv4Addr>()
        .is_ok_and(|ip| ip.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: link messages: a world's WebSocket endpoint is `wss://` to any host, or `ws://` only
    /// to this machine; nothing else is kept.
    #[test]
    fn a_websocket_endpoint_is_wss_or_plain_only_to_this_machine() {
        for kept in [
            "wss://play.example.org/ws",
            "wss://play.example.org:9443/",
            "WSS://Play.Example.org",
            "wss://[2001:db8::1]:9443/",
            "ws://127.0.0.1:9180/",
            "ws://127.0.0.2:9180",
            "ws://localhost:9180/",
            "ws://[::1]:9180/",
        ] {
            assert!(is_websocket_url(kept), "{kept}");
        }
        for refused in [
            "",
            "ws://play.example.org/ws",
            "ws://192.168.1.5:9180/",
            "ws://127.0.0.1.example.org/",
            "ws://127.0.0.1@example.org/",
            "wss://user@play.example.org/",
            "wss://",
            "wss:///ws",
            "wss://play.example.org:0/",
            "wss://play.example.org:99999/",
            "wss://play.example.org:/",
            "wss://play example.org/",
            "wss://play.example.org/\"",
            "wss://play_example.org/",
            "wss://[::1/",
            "https://play.example.org/ws",
            "javascript:alert(1)",
            "127.0.0.1:9000",
        ] {
            assert!(!is_websocket_url(refused), "{refused}");
        }
        assert!(is_websocket_url(&format!(
            "wss://a.example.org/{}",
            "x".repeat(MAX_URL_BYTES - 20)
        )));
        assert!(!is_websocket_url(&format!(
            "wss://a.example.org/{}",
            "x".repeat(MAX_URL_BYTES - 19)
        )));
    }

    /// Spec: link messages: a world's status document is `https://` to any host, or `http://` only
    /// to this machine.
    #[test]
    fn a_status_document_is_https_or_plain_only_to_this_machine() {
        assert!(is_status_url("https://play.example.org/status"));
        assert!(is_status_url("http://127.0.0.1:9100/status"));
        assert!(is_status_url("http://[::1]:9100/status"));
        assert!(!is_status_url("http://play.example.org/status"));
        assert!(!is_status_url("wss://play.example.org/status"));
        assert!(!is_status_url("https://user:pw@play.example.org/status"));
    }
}
