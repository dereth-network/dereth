//! A world's plugin policy: which automation domains it allows.
//!
//! A world may publish a policy saying, per automation domain, whether a plugin may act in it with
//! no player gesture behind the action (`allow`), only with one (`interactive-only`), or not at
//! all (`deny`); the same for a plugin's logging in and out (`session`); how many clients on one
//! machine a plugin may coordinate; and a note for the player. A world that publishes none gets
//! [`WorldPolicy::unpublished`]: every domain and session control allowed, with no limits.

use std::collections::BTreeMap;

use crate::permission::AutoDomain;

/// What a world allows in one automation domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainRule {
    /// Actions with or without a gesture.
    Allow,
    /// Actions only with a live activation (a gesture on the plugin's own surface).
    InteractiveOnly,
    /// No actions in this domain.
    Deny,
}

impl DomainRule {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::InteractiveOnly => "interactive-only",
            Self::Deny => "deny",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        [Self::Allow, Self::InteractiveOnly, Self::Deny]
            .into_iter()
            .find(|r| r.name() == s)
    }
}

/// A world's policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldPolicy {
    /// The rule for each domain the policy names.
    pub automation: BTreeMap<AutoDomain, DomainRule>,
    /// The rule for a domain it does not name.
    pub default: DomainRule,
    /// The rule for logging in, choosing a character and logging out without the player.
    pub session: DomainRule,
    /// Whether the policy was published by the world, or is the built-in one.
    pub published: bool,
    /// The world the policy names, when it names one.
    pub world: Option<String>,
    /// How many other clients on the machine a plugin may hear from (`None`: no limit).
    pub peers_local: Option<u32>,
    /// Whether automation stays allowed while the character is a player killer.
    pub pk_automation: bool,
    /// Whether plugins see only what the radar shows.
    pub read_parity: bool,
    /// The world's note to the player, and where its rules are.
    pub notes: Option<String>,
    pub notes_url: Option<String>,
}

impl WorldPolicy {
    /// The policy of a world that publishes none: every domain allowed, with or without a
    /// gesture, and session control too.
    #[must_use]
    pub fn unpublished() -> Self {
        Self {
            automation: BTreeMap::new(),
            default: DomainRule::Allow,
            session: DomainRule::Allow,
            published: false,
            world: None,
            peers_local: None,
            pk_automation: true,
            read_parity: false,
            notes: None,
            notes_url: None,
        }
    }

    /// A published policy with these rules and every other field at its published default: a
    /// domain it does not name is denied, and so is session control.
    #[must_use]
    pub fn published(automation: BTreeMap<AutoDomain, DomainRule>) -> Self {
        Self {
            automation,
            default: DomainRule::Deny,
            session: DomainRule::Deny,
            published: true,
            pk_automation: false,
            ..Self::unpublished()
        }
    }

    /// The rule for `domain`.
    #[must_use]
    pub fn rule(&self, domain: AutoDomain) -> DomainRule {
        self.automation
            .get(&domain)
            .copied()
            .unwrap_or(self.default)
    }

    /// Whether a plugin may log in, choose a character or log out: `interactive` says whether a
    /// gesture is behind it.
    #[must_use]
    pub fn allows_session(&self, interactive: bool) -> bool {
        match self.session {
            DomainRule::Allow => true,
            DomainRule::InteractiveOnly => interactive,
            DomainRule::Deny => false,
        }
    }

    /// The policy as lines for the player: where it comes from, then each domain's rule, then
    /// the note.
    #[must_use]
    pub fn describe(&self) -> Vec<String> {
        let mut out = vec![if self.published {
            format!(
                "World policy{}: published by the world.",
                self.world
                    .as_deref()
                    .map(|w| format!(" of {w}"))
                    .unwrap_or_default()
            )
        } else {
            "World policy: none published; everything is allowed.".to_owned()
        }];
        let rules: Vec<String> = AutoDomain::ALL
            .into_iter()
            .map(|d| format!("{} {}", d.name(), self.rule(d).name()))
            .chain([format!("session {}", self.session.name())])
            .collect();
        out.push(rules.join(", "));
        if let Some(n) = self.peers_local {
            out.push(format!("Other clients a plugin may coordinate: {n}."));
        }
        if let Some(t) = &self.notes {
            out.push(t.clone());
        }
        if let Some(u) = &self.notes_url {
            out.push(u.clone());
        }
        out
    }

    /// Whether an action in `domain` may run: `interactive` says whether a gesture is behind it.
    #[must_use]
    pub fn allows(&self, domain: AutoDomain, interactive: bool) -> bool {
        match self.rule(domain) {
            DomainRule::Allow => true,
            DomainRule::InteractiveOnly => interactive,
            DomainRule::Deny => false,
        }
    }
}

impl Default for WorldPolicy {
    fn default() -> Self {
        Self::unpublished()
    }
}

#[cfg(test)]
mod tests {
    use super::{DomainRule, WorldPolicy};
    use crate::permission::AutoDomain;

    #[test]
    fn a_world_with_no_policy_allows_every_domain_and_session_control_without_a_gesture() {
        let p = WorldPolicy::unpublished();
        for d in AutoDomain::ALL {
            assert!(p.allows(d, false), "{d:?} automated");
            assert!(p.allows(d, true), "{d:?} with a gesture");
        }
        assert!(p.allows_session(false));
        assert!(!p.published);
        assert_eq!(p.peers_local, None);
    }

    #[test]
    fn a_published_policy_denies_what_it_does_not_name_and_session_control() {
        let published = WorldPolicy::published(
            [
                (AutoDomain::Loot, DomainRule::Allow),
                (AutoDomain::Nav, DomainRule::InteractiveOnly),
            ]
            .into_iter()
            .collect(),
        );
        assert!(
            !published.allows(AutoDomain::Combat, true),
            "unnamed is denied"
        );
        assert!(published.allows(AutoDomain::Loot, false));
        assert!(!published.allows(AutoDomain::Nav, false));
        assert!(published.allows(AutoDomain::Nav, true));
        assert!(!published.allows_session(true));
    }

    #[test]
    fn a_denied_domain_refuses_even_a_gesture_and_the_rule_names_round_trip() {
        let mut p = WorldPolicy::unpublished();
        p.automation.insert(AutoDomain::Combat, DomainRule::Deny);
        assert!(!p.allows(AutoDomain::Combat, true));
        for r in [
            DomainRule::Allow,
            DomainRule::InteractiveOnly,
            DomainRule::Deny,
        ] {
            assert_eq!(DomainRule::parse(r.name()), Some(r));
        }
        assert_eq!(DomainRule::parse("maybe"), None);
    }
}
