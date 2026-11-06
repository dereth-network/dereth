//! What a world shows for the hub's answer to its operator's verification, one line whichever way
//! the answer came, in one server's words as in another's. The one line that differs is the refusal
//! of the world's key, which names where each server sets it: the caller's `key_unknown`.

/// The most of the hub's own words a line shows.
pub const MAX_MESSAGE_CHARS: usize = 200;

/// The line a verification that could not be sent at all is shown as.
pub const NOT_SENT: &str = "The verification could not be sent.";

/// What the hub's answers that the world is verified are shown as: with no approval named, then
/// approved, the day's allowance of new worlds spent, a slug that cannot join, and locked.
const VERIFIED: &str = "The Dereth Network verified this world.";
const APPROVED: &str = "The Dereth Network verified this world. It is on the network now; the \
                        link comes up within a minute.";
const RATE_LIMITED: &str = "The Dereth Network verified this world. Today's allowance of new \
                            worlds is spent: turn federation on from the site later.";
const NOT_NETWORK_SLUG: &str = "The Dereth Network verified this world. Its slug is too long, or \
                                reserved, to join the network.";
const LOCKED: &str =
    "The Dereth Network verified this world, but the hub's operator has locked it.";

/// The line the hub's answer to a verification (its status and its JSON body) is shown as;
/// `key_unknown` for a refusal of the world's key.
#[must_use]
pub fn answer_line(status: u16, body: &str, key_unknown: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    if status == 200 {
        return match v["approval"].as_str() {
            Some("approved") => APPROVED,
            Some("rate_limited") => RATE_LIMITED,
            Some("not_network_slug") => NOT_NETWORK_SLUG,
            Some("locked") => LOCKED,
            _ => VERIFIED,
        }
        .to_owned();
    }
    let message = v["error"]["message"].as_str().map(|m| {
        m.chars()
            .filter(|c| !c.is_control())
            .take(MAX_MESSAGE_CHARS)
            .collect::<String>()
    });
    match (status, message) {
        (401, _) => key_unknown.to_owned(),
        (429, _) => "The Dereth Network did not verify this world: too many attempts. Wait a \
                     while and try again."
            .to_owned(),
        // The hub's own words, ended as a sentence unless they already are.
        (_, Some(m)) if m.ends_with(['.', '?', '!']) => {
            format!("The Dereth Network did not verify this world: {m}")
        }
        (_, Some(m)) => format!("The Dereth Network did not verify this world: {m}."),
        (s, None) => format!("The Dereth Network did not verify this world: the hub answered {s}."),
    }
}

/// The line a verification the hub could not be reached for is shown as, with why.
#[must_use]
pub fn unreachable_line(why: &str) -> String {
    format!("The Dereth Network's hub could not be reached: {why}.")
}

#[cfg(test)]
mod tests {
    use super::answer_line;

    /// Spec: world behaviour: the hub's answer to a verification is one line for whoever typed it;
    /// only a refusal of the world's key names where this server sets it.
    #[test]
    fn a_refusal_of_the_key_is_the_servers_own_line_and_nothing_else_is() {
        assert_eq!(answer_line(401, "{}", "its own words"), "its own words");
        for status in [200, 429, 422, 503] {
            assert!(!answer_line(status, "{}", "its own words").contains("its own words"));
        }
    }
}
