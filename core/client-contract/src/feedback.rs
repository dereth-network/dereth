//! Meaning supplied by the operation that produces a displayed line.

/// The visual and audible emphasis of a viewport notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackSeverity {
    Information,
    Warning,
}

/// Where a displayed line originated, independent of its channel or wording.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackKind {
    #[default]
    Ordinary,
    Local,
    NumericFailure(u32),
    ServerTransient,
}

/// Operation metadata carried beside text throughout delivery.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Feedback {
    pub kind: FeedbackKind,
    pub severity: Option<FeedbackSeverity>,
}
impl Feedback {
    pub const ORDINARY: Self = Self {
        kind: FeedbackKind::Ordinary,
        severity: None,
    };
    pub const LOCAL: Self = Self {
        kind: FeedbackKind::Local,
        severity: None,
    };
    pub const INFORMATION: Self = Self::local(FeedbackSeverity::Information);
    pub const WARNING: Self = Self::local(FeedbackSeverity::Warning);
    pub const SERVER_TRANSIENT: Self = Self {
        kind: FeedbackKind::ServerTransient,
        severity: Some(FeedbackSeverity::Warning),
    };

    #[must_use]
    pub const fn local(severity: FeedbackSeverity) -> Self {
        Self {
            kind: FeedbackKind::Local,
            severity: Some(severity),
        }
    }
    #[must_use]
    pub const fn numeric(code: u32) -> Self {
        Self {
            kind: FeedbackKind::NumericFailure(code),
            severity: match code {
                0x403 | 0x404 | 0x407 | 0x52a => Some(FeedbackSeverity::Warning),
                _ => None,
            },
        }
    }
}

#[cfg(test)]
mod tests;
