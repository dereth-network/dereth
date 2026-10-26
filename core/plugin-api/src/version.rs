//! The API version and the rule that decides whether a host runs a plugin.

use std::fmt;

/// An API version, `MAJOR.MINOR`.
///
/// A host runs a plugin when the majors are equal and the host's minor is at least the plugin's.
/// Minor versions only add; a major version may break. While the major is 0 the spec is a draft,
/// and the same rule applies to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApiVersion {
    pub major: u16,
    pub minor: u16,
}

impl ApiVersion {
    /// The version this build of the interface implements: spec draft 0.7.
    pub const CURRENT: Self = Self { major: 0, minor: 7 };

    /// Parse `"MAJOR.MINOR"`. Anything else, a patch number included, is refused.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let (a, b) = s.trim().split_once('.')?;
        if a.is_empty() || b.is_empty() || !a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit()) {
            return None;
        }
        Some(Self {
            major: a.parse().ok()?,
            minor: b.parse().ok()?,
        })
    }

    /// Whether a host implementing `self` runs a plugin that asks for `wanted`.
    #[must_use]
    pub fn runs(self, wanted: Self) -> bool {
        self.major == wanted.major && self.minor >= wanted.minor
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

#[cfg(test)]
mod tests {
    use super::ApiVersion;

    #[test]
    fn a_host_runs_a_plugin_of_its_major_and_no_newer_minor() {
        let host = ApiVersion { major: 1, minor: 3 };
        assert!(host.runs(ApiVersion { major: 1, minor: 0 }));
        assert!(host.runs(ApiVersion { major: 1, minor: 3 }));
        assert!(!host.runs(ApiVersion { major: 1, minor: 4 }));
        assert!(!host.runs(ApiVersion { major: 0, minor: 1 }));
        assert!(!host.runs(ApiVersion { major: 2, minor: 0 }));
    }

    #[test]
    fn only_major_dot_minor_parses() {
        assert_eq!(
            ApiVersion::parse("0.1"),
            Some(ApiVersion { major: 0, minor: 1 })
        );
        assert_eq!(
            ApiVersion::parse(" 12.40 "),
            Some(ApiVersion {
                major: 12,
                minor: 40
            })
        );
        for bad in ["1", "1.2.3", "a.b", "1.", ".1", "-1.0", ""] {
            assert_eq!(ApiVersion::parse(bad), None, "{bad:?}");
        }
    }
}
