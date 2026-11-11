//! Who refused a world before its link's upgrade or its heartbeat (`docs/network/02-link-protocol.md`
//! section 1). The hub marks each of its own refusals with [`HEADER`]; a `401` or `403` without it
//! came from something in front of the hub (a reverse proxy, or a service such as Cloudflare's
//! that challenges what it takes for a bot), never from the hub. A world reads which it was with
//! [`RefusedBy::read`] and logs [`RefusedBy::describe`].

/// The response header the hub marks its own refusals with: [`KEY`] or [`FEDERATION`].
pub const HEADER: &str = "X-Dereth-Hub-Refusal";

/// The mark of a refusal of the world's key (`401`).
pub const KEY: &str = "key";

/// The mark of a refusal of a world whose federation flag is not `on` (`403`).
pub const FEDERATION: &str = "federation";

/// The most of a header's value a world keeps from a refusal, so that a long one from anything in
/// front of the hub never fills its log.
const MAX_VALUE: usize = 64;

/// Why the hub said it refused the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubRefusal {
    /// The world's key ([`KEY`]).
    Key,
    /// The world's federation flag is not `on` ([`FEDERATION`]).
    Federation,
    /// A mark this world does not know, as the hub gave it.
    Other(String),
}

/// Who refused the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefusedBy {
    /// The hub itself: the answer carried its mark.
    Hub(HubRefusal),
    /// Something in front of the hub: the answer carried no mark. What it named itself as, from
    /// its `Server` header, and Cloudflare's id for the request (`CF-Ray`), when it gave them.
    Front {
        server: Option<String>,
        cf_ray: Option<String>,
    },
}

impl RefusedBy {
    /// Who refused, from the refusal's headers: `header` gives the value of the header it is
    /// asked for by name, if the answer had one.
    pub fn read<'a>(header: impl Fn(&str) -> Option<&'a str>) -> Self {
        match header(HEADER).map(str::trim) {
            Some(KEY) => Self::Hub(HubRefusal::Key),
            Some(FEDERATION) => Self::Hub(HubRefusal::Federation),
            Some(other) if !other.is_empty() => Self::Hub(HubRefusal::Other(kept(other))),
            _ => Self::Front {
                server: header("server").map(kept).filter(|v| !v.is_empty()),
                cf_ray: header("cf-ray").map(kept).filter(|v| !v.is_empty()),
            },
        }
    }

    /// What a world logs for this refusal of `status`, without what it does next.
    #[must_use]
    pub fn describe(&self, status: u16) -> String {
        match self {
            Self::Hub(HubRefusal::Key) => format!("the hub refused this world's key ({status})"),
            Self::Hub(HubRefusal::Federation) => {
                format!("the hub refused this world ({status}: its federation flag is not on)")
            }
            Self::Hub(HubRefusal::Other(why)) => {
                format!("the hub refused this world ({status}: {why})")
            }
            Self::Front { server, cf_ray } => {
                let mut named = String::new();
                if let Some(s) = server {
                    named.push_str(&format!(", server: {s}"));
                }
                if let Some(r) = cf_ray {
                    named.push_str(&format!(", cf-ray: {r}"));
                }
                format!(
                    "something in front of the hub refused the connection (HTTP {status}{named}); \
                     check the proxy or Cloudflare (Bot Fight Mode challenges game servers)"
                )
            }
        }
    }
}

/// A header's value as a world keeps it: printable ASCII only, at most [`MAX_VALUE`] bytes.
fn kept(v: &str) -> String {
    v.trim()
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(MAX_VALUE)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(headers: &[(&str, &str)]) -> RefusedBy {
        RefusedBy::read(|name| {
            headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| *v)
        })
    }

    /// Spec: link protocol: a refusal with the hub's mark is the hub's, and names why.
    #[test]
    fn a_marked_refusal_is_the_hubs_own() {
        assert_eq!(
            read(&[("x-dereth-hub-refusal", "key")]),
            RefusedBy::Hub(HubRefusal::Key)
        );
        let fed = read(&[("X-Dereth-Hub-Refusal", "federation"), ("Server", "x")]);
        assert_eq!(fed, RefusedBy::Hub(HubRefusal::Federation));
        assert_eq!(
            fed.describe(403),
            "the hub refused this world (403: its federation flag is not on)"
        );
        assert_eq!(
            read(&[(HEADER, "a-later-reason")]).describe(403),
            "the hub refused this world (403: a-later-reason)"
        );
    }

    /// Spec: link protocol: a 401 or 403 without the hub's mark came from in front of the hub,
    /// and names what answered.
    #[test]
    fn an_unmarked_refusal_came_from_in_front_of_the_hub() {
        let by = read(&[("Server", "cloudflare"), ("CF-RAY", "8f00aa-AMS")]);
        assert_eq!(
            by,
            RefusedBy::Front {
                server: Some("cloudflare".into()),
                cf_ray: Some("8f00aa-AMS".into()),
            }
        );
        let line = by.describe(403);
        assert!(line.starts_with("something in front of the hub refused the connection (HTTP 403, server: cloudflare, cf-ray: 8f00aa-AMS)"), "{line}");
        assert!(!line.contains("federation"), "{line}");
        assert_eq!(
            read(&[]).describe(401),
            "something in front of the hub refused the connection (HTTP 401); check the proxy or \
             Cloudflare (Bot Fight Mode challenges game servers)"
        );
        assert!(matches!(read(&[(HEADER, " ")]), RefusedBy::Front { .. }));
    }

    /// Spec: link protocol: what a world keeps of a refusal's headers is short and printable.
    #[test]
    fn a_refusals_header_values_are_kept_short_and_printable() {
        let long = "z".repeat(500);
        let RefusedBy::Front { server, .. } = read(&[("server", &long)]) else {
            panic!("unmarked");
        };
        assert_eq!(server.map(|s| s.len()), Some(MAX_VALUE));
        let RefusedBy::Hub(HubRefusal::Other(why)) = read(&[(HEADER, "a\u{1b}[31mb\u{7f}")]) else {
            panic!("marked");
        };
        assert_eq!(why, "a[31mb");
    }
}
