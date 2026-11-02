//! Which browser pages may open the relay.
//!
//! - **On the player's machine**, a page served from that machine (`localhost`, `127.0.0.1`,
//!   `[::1]`) and the one page origin the player names; a request with no `Origin` is not from a
//!   browser page, and is accepted, as the player's own programs may.
//! - **Beside a server**, only the origins named: the Dereth web client's, and any the operator
//!   adds. A request with no `Origin` is refused: every browser sends one with a
//!   WebSocket handshake, so one without is a program, and a program can speak UDP to the server
//!   itself. The relay would only lend it its own address, the one the game server sees and bans.

/// The Dereth web client's page origin, which a relay beside a server always admits.
pub const WEB_CLIENT: &str = "https://play.dereth.network";

/// A page origin as it is named: `https://` or `http://`, a host and an optional port, nothing
/// after them. Kept without a trailing slash.
///
/// # Errors
/// Anything else, with why.
pub fn page_origin(value: &str) -> Result<String, String> {
    let value = value.trim().trim_end_matches('/');
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .ok_or_else(|| format!("{value}: an origin is https://host or http://host[:port]"))?;
    if rest.is_empty()
        || rest.contains(['/', '?', '#', '@'])
        || !rest.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err(format!(
            "{value}: an origin has a host and a port and nothing else"
        ));
    }
    Ok(value.to_owned())
}

/// Whether `origin` is one of `named`, compared without case and without a trailing slash.
fn named(origin: &str, named: &[String]) -> bool {
    let origin = origin.trim_end_matches('/');
    named.iter().any(|n| n.eq_ignore_ascii_case(origin))
}

/// A page the relay on the player's machine admits: no origin (not a browser page), one served
/// from this machine, or the one the player named.
#[must_use]
pub fn local_allows(origin: Option<&str>, player_named: Option<&str>) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    if player_named.is_some_and(|n| named(origin, &[n.to_owned()])) {
        return true;
    }
    let host = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .unwrap_or("");
    let host = match host.rsplit_once(':') {
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host,
    };
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

/// A page a relay beside a server admits: one of the origins named, and nothing else, a request
/// with no origin included.
#[must_use]
pub fn public_allows(origin: Option<&str>, allowed: &[String]) -> bool {
    origin.is_some_and(|o| named(o, allowed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_the_players_machine_only_a_local_page_or_the_named_one_may_open_it() {
        assert!(local_allows(None, None));
        assert!(local_allows(Some("http://127.0.0.1:8080"), None));
        assert!(local_allows(Some("http://localhost"), None));
        assert!(!local_allows(Some("https://example.com"), None));
        assert!(!local_allows(
            Some("http://127.0.0.1.example.com:8080"),
            None
        ));
        assert!(!local_allows(Some("null"), None));
        let named = Some("https://play.example.org");
        assert!(local_allows(Some("https://PLAY.example.org/"), named));
        assert!(!local_allows(Some("http://play.example.org"), named));
        assert!(!local_allows(
            Some("https://play.example.org.evil.example"),
            named
        ));
    }

    /// Beside a server, a page is admitted only by name: no local page, and no request without an
    /// origin.
    #[test]
    fn beside_a_server_only_a_named_origin_may_open_it() {
        let allowed = vec![WEB_CLIENT.to_owned(), "https://play.example.org".to_owned()];
        assert!(public_allows(Some("https://play.dereth.network"), &allowed));
        assert!(public_allows(Some("https://Play.Example.org/"), &allowed));
        assert!(!public_allows(None, &allowed), "no origin: not a browser");
        assert!(!public_allows(Some("http://127.0.0.1:8080"), &allowed));
        assert!(!public_allows(Some("http://play.dereth.network"), &allowed));
        assert!(!public_allows(
            Some("https://play.dereth.network.evil.example"),
            &allowed
        ));
        assert!(!public_allows(Some("null"), &allowed));
    }

    #[test]
    fn an_origin_is_a_scheme_a_host_and_a_port() {
        assert_eq!(
            page_origin("https://play.example.org/"),
            Ok("https://play.example.org".to_owned())
        );
        assert!(page_origin("http://127.0.0.1:8080").is_ok());
        for bad in [
            "play.example.org",
            "https://",
            "https://a.example/play",
            "https://a.example?x",
            "https://user@a.example",
            "wss://a.example",
        ] {
            assert!(page_origin(bad).is_err(), "{bad}");
        }
    }
}
