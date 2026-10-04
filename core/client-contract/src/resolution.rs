//! Screen-size requests and the prompt projected by either interface.

/// The prompt sequence chosen when the player changes the screen size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionPolicy {
    Classic,
    Modern,
}

/// An explicit screen-size gesture. Ordinary preference writes remain noninteractive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionAction {
    Begin {
        size: (u32, u32),
        policy: ResolutionPolicy,
        persist: bool,
    },
    Answer {
        token: u64,
        yes: bool,
    },
    Dismiss {
        token: u64,
    },
    /// The active presenter could not construct the prompt.
    Unavailable {
        token: u64,
    },
}

/// What the active interface draws; deadlines belong to the runtime.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolutionPrompt {
    pub token: u64,
    pub kind: ResolutionPromptKind,
    pub deadline: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionPromptKind {
    OfferTest,
    Accept,
    ApplyFailed,
    RevertFailed,
    Reset,
}
