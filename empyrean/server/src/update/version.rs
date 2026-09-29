//! Release versions: `MAJOR.MINOR.PATCH`, optionally with a pre-release suffix (`-rc.1`), ordered
//! as semantic versioning orders them (a pre-release sorts before its release).

use std::cmp::Ordering;
use std::fmt;

/// The prefix of every Empyrean release tag (`empyrean-v0.1.0`).
pub const TAG_PREFIX: &str = "empyrean-v";

/// A release version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// The pre-release suffix without its `-` (`rc.1`), when there is one.
    pub pre: Option<String>,
}

impl Version {
    /// Reads `1.2.3` or `1.2.3-rc.1` (a leading `v` is allowed).
    ///
    /// # Errors
    /// When `text` is not such a version.
    pub fn parse(text: &str) -> Result<Self, String> {
        let t = text.trim();
        let t = t.strip_prefix('v').unwrap_or(t);
        let (core, pre) = match t.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (t, None),
        };
        let bad = || format!("`{text}` is not a version such as 0.1.0 or 0.2.0-rc.1");
        let mut parts = core.split('.');
        let mut num = || -> Result<u64, String> {
            let p = parts.next().ok_or_else(bad)?;
            if p.is_empty()
                || !p.bytes().all(|b| b.is_ascii_digit())
                || (p.len() > 1 && p.starts_with('0'))
            {
                return Err(bad());
            }
            p.parse().map_err(|_| bad())
        };
        let (major, minor, patch) = (num()?, num()?, num()?);
        if parts.next().is_some() {
            return Err(bad());
        }
        if let Some(p) = pre {
            if p.is_empty()
                || p.split('.').any(|id| {
                    id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
            {
                return Err(bad());
            }
        }
        Ok(Self {
            major,
            minor,
            patch,
            pre: pre.map(str::to_owned),
        })
    }

    /// The version a release tag names (`empyrean-v0.1.0`); `None` for any other tag.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        Self::parse(tag.strip_prefix(TAG_PREFIX)?).ok()
    }

    /// This build's version.
    #[must_use]
    pub fn this_build() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("the crate's version is a release version")
    }

    /// Whether it is a pre-release.
    #[must_use]
    pub fn is_prerelease(&self) -> bool {
        self.pre.is_some()
    }

    /// The release tag (`empyrean-v0.1.0`).
    #[must_use]
    pub fn tag(&self) -> String {
        format!("{TAG_PREFIX}{self}")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(p) = &self.pre {
            write!(f, "-{p}")?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => compare_pre(a, b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Pre-release precedence: identifier by identifier, numbers numerically and below words, and a
/// shorter list first when all else is equal.
fn compare_pre(a: &str, b: &str) -> Ordering {
    let mut x = a.split('.');
    let mut y = b.split('.');
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) => {
                let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => p.cmp(q),
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}
