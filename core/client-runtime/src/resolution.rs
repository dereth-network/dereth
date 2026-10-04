//! Screen-size changes survive interface replacement and await the displayed host result.
use dereth_client_contract::resolution::{
    ResolutionAction, ResolutionPolicy, ResolutionPrompt, ResolutionPromptKind,
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    Offer,
    Applying { test: bool, since: f64 },
    Delay { ticks: u8 },
    Confirm { deadline: f64 },
    Reverting { since: f64, expired: bool },
}
#[derive(Clone, Copy, Debug)]
struct Change {
    previous: (u32, u32),
    target: (u32, u32),
    policy: ResolutionPolicy,
    persist: bool,
    stage: Stage,
}

fn expired(policy: ResolutionPolicy, now: f64, deadline: f64) -> bool {
    match policy {
        ResolutionPolicy::Classic => now >= deadline,
        ResolutionPolicy::Modern => now > deadline,
    }
}

/// One pending resize and its response token. It contains no window or preference store.
#[derive(Debug, Default)]
pub struct ResolutionTransaction {
    next: u64,
    token: u64,
    request: u64,
    change: Option<Change>,
    result: Option<ResolutionPromptKind>,
    interface: Option<bool>,
    completion: Option<ResolutionCompletion>,
}
/// A completed choice, read once by adapters that keep a page draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolutionCompletion {
    pub serial: u64,
    pub size: (u32, u32),
    pub save: bool,
}
/// A requested resize or a preference-only reconciliation after the host has answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolutionEffect {
    pub size: (u32, u32),
    pub resize: bool,
    pub store: bool,
}
impl ResolutionEffect {
    fn resize(size: (u32, u32), store: bool) -> Self {
        Self {
            size,
            resize: true,
            store,
        }
    }
    fn reconcile(size: (u32, u32)) -> Self {
        Self {
            size,
            resize: false,
            store: true,
        }
    }
}
impl ResolutionTransaction {
    fn finish(&mut self, size: (u32, u32), save: bool) {
        let serial = self.completion.map_or(1, |c| c.serial.wrapping_add(1));
        self.completion = Some(ResolutionCompletion { serial, size, save });
        self.change = None;
    }
    /// The latest completed transaction; reading it does not consume it.
    pub fn completion(&self) -> Option<ResolutionCompletion> {
        self.completion
    }

    fn renew_token(&mut self) {
        self.next = self.next.wrapping_add(1);
        self.token = self.next;
    }
    /// Replacing the presenter invalidates its answers without changing any deadline.
    pub fn interface(&mut self, classic: bool) {
        if self.interface != Some(classic) {
            self.interface = Some(classic);
            self.renew_token();
        }
    }
    /// A prompt is only a projection of this transaction.
    pub fn prompt(&self) -> Option<ResolutionPrompt> {
        let (kind, deadline) = if let Some(kind) = self.result {
            (kind, None)
        } else {
            match self.change?.stage {
                Stage::Offer => (ResolutionPromptKind::OfferTest, None),
                Stage::Confirm { deadline } => (ResolutionPromptKind::Accept, Some(deadline)),
                _ => return None,
            }
        };
        Some(ResolutionPrompt {
            token: self.token,
            kind,
            deadline,
        })
    }
    /// The host request currently awaiting a size report.
    pub fn awaited(&self) -> Option<(u64, (u32, u32))> {
        let c = self.change?;
        match c.stage {
            Stage::Applying { .. } => Some((self.request, c.target)),
            Stage::Reverting { .. } => Some((self.request, c.previous)),
            _ => None,
        }
    }
    /// The actual size captured before the pending test began.
    pub fn previous(&self) -> Option<(u32, u32)> {
        self.change.map(|c| c.previous)
    }
    /// Whether a size test is still in progress.
    pub fn pending(&self) -> bool {
        self.change.is_some()
    }
    /// Whether the initiating page asked to retain the confirmed size in its saved snapshot.
    pub fn save_intent(&self) -> bool {
        self.change.is_some_and(|c| c.persist)
    }
    /// Start or answer a transaction. A returned size must be sent through the normal host path.
    pub fn action(
        &mut self,
        action: ResolutionAction,
        actual: (u32, u32),
        now: f64,
    ) -> Option<ResolutionEffect> {
        match action {
            ResolutionAction::Begin {
                size,
                policy,
                persist,
            } => {
                if self.change.is_some() || size == actual || size.0 < 800 || size.1 < 600 {
                    return None;
                }
                self.renew_token();
                self.result = None;
                let stage = if policy == ResolutionPolicy::Classic {
                    Stage::Offer
                } else {
                    Stage::Applying {
                        test: true,
                        since: now,
                    }
                };
                self.change = Some(Change {
                    previous: actual,
                    target: size,
                    policy,
                    persist,
                    stage,
                });
                if policy == ResolutionPolicy::Modern {
                    self.request = self.request.wrapping_add(1);
                    Some(ResolutionEffect::resize(size, true))
                } else {
                    None
                }
            }
            ResolutionAction::Unavailable { token } => {
                if token != self.token {
                    return None;
                }
                let c = self.change?;
                if c.stage == Stage::Offer {
                    return self.cancel();
                }
                self.action(ResolutionAction::Answer { token, yes: false }, actual, now)
            }
            ResolutionAction::Dismiss { token } => {
                if token == self.token {
                    self.result = None;
                }
                None
            }
            ResolutionAction::Answer { token, yes } => {
                if token != self.token {
                    return None;
                }
                let c = self.change?;
                match c.stage {
                    Stage::Offer => {
                        self.renew_token();
                        self.request = self.request.wrapping_add(1);
                        self.change.as_mut().unwrap().stage = Stage::Applying {
                            test: yes,
                            since: now,
                        };
                        Some(ResolutionEffect::resize(c.target, false))
                    }
                    Stage::Confirm { deadline } if yes && !expired(c.policy, now, deadline) => {
                        self.finish(c.target, c.persist);
                        (c.persist || c.policy == ResolutionPolicy::Modern)
                            .then_some(ResolutionEffect::reconcile(c.target))
                    }
                    Stage::Confirm { deadline } => {
                        self.renew_token();
                        self.request = self.request.wrapping_add(1);
                        self.change.as_mut().unwrap().stage = Stage::Reverting {
                            since: now,
                            expired: expired(c.policy, now, deadline),
                        };
                        Some(ResolutionEffect::resize(c.previous, true))
                    }
                    _ => None,
                }
            }
        }
    }
    /// An observed host result. Wrong-size asynchronous observations may still complete later.
    pub fn host_result(
        &mut self,
        token: u64,
        actual: (u32, u32),
        failed: bool,
        now: f64,
    ) -> Option<ResolutionEffect> {
        if token != self.request {
            return None;
        }
        let c = self.change?;
        match c.stage {
            Stage::Applying { test, .. } if actual == c.target && !failed => {
                if !test {
                    self.finish(c.target, c.persist);
                    return c.persist.then_some(ResolutionEffect::reconcile(c.target));
                } else {
                    self.renew_token();
                    let stage = if c.policy == ResolutionPolicy::Modern {
                        Stage::Delay { ticks: 0 }
                    } else {
                        Stage::Confirm {
                            deadline: now + 15.0,
                        }
                    };
                    self.change.as_mut().unwrap().stage = stage;
                }
                None
            }
            Stage::Applying { .. } if failed => {
                self.finish(actual, false);
                self.result = Some(ResolutionPromptKind::ApplyFailed);
                Some(ResolutionEffect::reconcile(actual))
            }
            Stage::Reverting { expired, .. } if actual == c.previous && !failed => {
                self.finish(actual, false);
                self.result = (expired && c.policy == ResolutionPolicy::Classic)
                    .then_some(ResolutionPromptKind::Reset);
                None
            }
            Stage::Reverting { .. } if failed => {
                self.finish(actual, true);
                self.result = Some(ResolutionPromptKind::RevertFailed);
                Some(ResolutionEffect::reconcile(actual))
            }
            _ => None,
        }
    }
    /// Advance once per frame, independently of whichever interface is visible.
    pub fn tick(&mut self, now: f64, actual: (u32, u32)) -> Option<ResolutionEffect> {
        let c = self.change?;
        match c.stage {
            Stage::Applying { since, .. } | Stage::Reverting { since, .. }
                if now - since >= 2.0 =>
            {
                let (token, wanted) = self.awaited()?;
                self.host_result(token, actual, actual != wanted, now)
            }
            Stage::Delay { ticks } => {
                if ticks + 1 >= 2 {
                    self.renew_token();
                }
                self.change.as_mut().unwrap().stage = if ticks + 1 >= 2 {
                    Stage::Confirm {
                        deadline: now + 10.0,
                    }
                } else {
                    Stage::Delay { ticks: ticks + 1 }
                };
                None
            }
            Stage::Confirm { deadline } if expired(c.policy, now, deadline) => {
                self.renew_token();
                self.request = self.request.wrapping_add(1);
                self.change.as_mut().unwrap().stage = Stage::Reverting {
                    since: now,
                    expired: true,
                };
                Some(ResolutionEffect::resize(c.previous, true))
            }
            _ => None,
        }
    }
    /// A noninteractive restore or shutdown supersedes the pending choice.
    pub fn cancel(&mut self) -> Option<ResolutionEffect> {
        let old = self.change.map(|c| {
            self.finish(c.previous, false);
            ResolutionEffect::reconcile(c.previous)
        });
        self.result = None;
        self.renew_token();
        old
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const OLD: (u32, u32) = (800, 600);
    const NEW: (u32, u32) = (1024, 768);
    fn begin(
        t: &mut ResolutionTransaction,
        policy: ResolutionPolicy,
        now: f64,
    ) -> Option<ResolutionEffect> {
        t.action(
            ResolutionAction::Begin {
                size: NEW,
                policy,
                persist: true,
            },
            OLD,
            now,
        )
    }
    fn answer(t: &mut ResolutionTransaction, yes: bool, now: f64) -> Option<ResolutionEffect> {
        t.action(
            ResolutionAction::Answer {
                token: t.prompt().unwrap().token,
                yes,
            },
            NEW,
            now,
        )
    }

    /// Behaviour: presentation.resolution.shared-transaction
    #[test]
    fn host_results_and_prompt_answers_have_independent_lifetimes() {
        let mut t = ResolutionTransaction::default();
        t.interface(true);
        assert_eq!(begin(&mut t, ResolutionPolicy::Classic, 10.0), None);
        let offer = t.prompt().unwrap();
        assert_eq!(offer.kind, ResolutionPromptKind::OfferTest);
        assert_eq!(
            answer(&mut t, true, 11.0),
            Some(ResolutionEffect::resize(NEW, false))
        );
        assert!(t.prompt().is_none(), "queueing is not success");
        let apply = t.awaited().unwrap().0;
        t.interface(false);
        assert_eq!(
            t.awaited().unwrap().0,
            apply,
            "a presenter does not own host requests"
        );
        t.host_result(apply, NEW, false, 12.0);
        let confirm = t.prompt().unwrap();
        assert_eq!(confirm.deadline, Some(27.0));
        assert_ne!(confirm.token, offer.token);
        assert_eq!(
            t.action(
                ResolutionAction::Answer {
                    token: offer.token,
                    yes: true
                },
                NEW,
                13.0
            ),
            None
        );
        assert!(t.pending());
        t.interface(true);
        assert_eq!(t.prompt().unwrap().deadline, Some(27.0));
        assert_ne!(t.prompt().unwrap().token, confirm.token);
        assert_eq!(
            t.action(
                ResolutionAction::Answer {
                    token: confirm.token,
                    yes: true
                },
                NEW,
                14.0
            ),
            None
        );
        assert!(t.tick(26.999, NEW).is_none());
        assert_eq!(
            answer(&mut t, true, 27.0),
            Some(ResolutionEffect::resize(OLD, true)),
            "even a Yes at the deadline expires"
        );
        let revert = t.awaited().unwrap().0;
        assert_ne!(revert, apply);
        assert_eq!(
            t.host_result(apply, NEW, true, 27.5),
            None,
            "old apply refusal cannot refuse a revert"
        );
        assert!(
            t.tick(28.999, NEW).is_none(),
            "revert has a fresh two-second host deadline"
        );
        assert_eq!(t.tick(29.0, NEW), Some(ResolutionEffect::reconcile(NEW)));
        assert_eq!(t.prompt().unwrap().kind, ResolutionPromptKind::RevertFailed);
    }

    /// Behaviour: presentation.resolution.shared-transaction
    #[test]
    fn modern_waits_for_host_and_two_ticks_then_reverts_only_its_size() {
        let mut t = ResolutionTransaction::default();
        assert_eq!(
            begin(&mut t, ResolutionPolicy::Modern, 1.0),
            Some(ResolutionEffect::resize(NEW, true))
        );
        assert_eq!(
            begin(&mut t, ResolutionPolicy::Classic, 1.2),
            None,
            "pending choice wins"
        );
        let request = t.awaited().unwrap().0;
        t.host_result(request, (900, 700), false, 1.4);
        assert!(t.prompt().is_none());
        t.host_result(request, NEW, false, 1.5);
        t.tick(1.6, NEW);
        assert!(t.prompt().is_none());
        t.tick(1.7, NEW);
        assert_eq!(t.prompt().unwrap().deadline, Some(11.7));
        assert!(
            t.tick(11.7, NEW).is_none(),
            "Modern expiry is strictly after its deadline"
        );
        assert_eq!(
            t.tick(11.701, NEW),
            Some(ResolutionEffect::resize(OLD, true))
        );
        let revert = t.awaited().unwrap().0;
        assert_eq!(t.host_result(revert, OLD, false, 11.8), None);
        assert!(!t.pending());
        assert!(t.prompt().is_none());
        begin(&mut t, ResolutionPolicy::Modern, 13.0);
        assert!(t.tick(14.999, OLD).is_none());
        assert_eq!(t.tick(15.0, OLD), Some(ResolutionEffect::reconcile(OLD)));
        assert_eq!(t.prompt().unwrap().kind, ResolutionPromptKind::ApplyFailed);
    }

    /// Behaviour: presentation.resolution.shared-transaction
    #[test]
    fn classic_permanent_accept_and_missing_dialog_keep_explicit_save_boundaries() {
        let mut t = ResolutionTransaction::default();
        begin(&mut t, ResolutionPolicy::Classic, 1.0);
        assert_eq!(
            answer(&mut t, false, 2.0),
            Some(ResolutionEffect::resize(NEW, false))
        );
        assert!(t.pending());
        let request = t.awaited().unwrap().0;
        assert_eq!(
            t.host_result(request, NEW, false, 2.5),
            Some(ResolutionEffect::reconcile(NEW))
        );
        assert!(!t.pending());
        begin(&mut t, ResolutionPolicy::Classic, 3.0);
        let token = t.prompt().unwrap().token;
        assert_eq!(
            t.action(ResolutionAction::Unavailable { token }, OLD, 3.1),
            Some(ResolutionEffect::reconcile(OLD))
        );
        assert!(
            !t.pending(),
            "a missing offer cannot authorize a permanent resize"
        );
        begin(&mut t, ResolutionPolicy::Classic, 4.0);
        answer(&mut t, true, 4.0);
        let request = t.awaited().unwrap().0;
        t.host_result(request, NEW, false, 5.0);
        assert_eq!(
            answer(&mut t, true, 6.0),
            Some(ResolutionEffect::reconcile(NEW))
        );
        assert!(!t.pending());
        begin(&mut t, ResolutionPolicy::Modern, 7.0);
        assert_eq!(t.cancel(), Some(ResolutionEffect::reconcile(OLD)));
        assert!(t.prompt().is_none());
        assert!(!t.pending());
    }
}
