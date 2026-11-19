//! The game's questions to the player, kept for every front end ([`Questions`]): which notices
//! of the player's scripts ask one, what an answer does, and what takes one down.
//!
//! - **Asked.** The player died; someone offers to bring the player back (with no name, it reads
//!   as death's own question); an effect asks whether it may go on (one such question at a time:
//!   a new one takes the place of the last); the scripts' message to read; a fellowship's place
//!   offered, an oath offered, another player asking to trade (each while no question of its kind
//!   is open). Death's questions and the effect's go in front of the others, a message when the
//!   scripts say so.
//! - **Answered** ([`Action::AnswerQuestion`]). Death's question brings the player back; an offer
//!   to bring the player back, yes brings it back, no turns the offer down and asks death's
//!   question again; an effect's yes lets it go on; a fellowship's place and an oath are taken or
//!   turned down; a trade asked for is opened or closed. A message and an effect's no ask
//!   nothing of the game. The same actions sent another way (the game's own interface sends them
//!   itself) take the question down too, so every front end sees the same questions open.
//! - **Taken down.** Death's questions when the player lives again; a fellowship's place when the
//!   offer runs out; an oath when it was done; a trade asked for when the trade closes; every
//!   question when the player logs off or leaves the world.
//!
//! At most [`MOST`] are open: past that, the oldest message goes first, then the oldest question.

use auberean_client_contract::notice::{
    AllegianceNotice, DialogNotice, FellowshipNotice, Notice, PlayerNotice, Said, TradeNotice,
};
use auberean_client_contract::question::{Asked, Question};
use auberean_client_contract::text::{TextRef, TextValue, TextVariable};
use auberean_client_contract::{Action, SocialAction, TradeAction};

use crate::Client;

/// How many questions may be open at once.
pub const MOST: usize = 16;

/// The reasons an offer turned down is sent with: a fellowship's place, an oath.
pub const FELLOWSHIP_DECLINED: u32 = 0x138D;
pub const OATH_DECLINED: u32 = 0x1585;

/// The answer to a fellowship's place offered: taken (`yes`), or turned down with the reason the
/// game's own interface gives.
#[must_use]
pub fn fellowship_answer(yes: bool) -> SocialAction {
    if yes {
        SocialAction::AcceptRecruitment
    } else {
        SocialAction::DeclineRecruitment(FELLOWSHIP_DECLINED)
    }
}

/// The answer to an oath offered: taken (`yes`), or refused with the reason the game's own
/// interface gives.
#[must_use]
pub fn oath_answer(yes: bool) -> SocialAction {
    if yes {
        SocialAction::TakeOath
    } else {
        SocialAction::RefuseOath(OATH_DECLINED)
    }
}

/// A question open, before its words are written.
#[derive(Clone, Debug, PartialEq)]
struct Open {
    id: u32,
    asked: Asked,
    at_once: bool,
}

/// The game's questions open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Questions {
    /// The last number given (a question's number is never 0).
    last: u32,
    open: Vec<Open>,
}

/// The kind of a question, to find those of a kind.
fn same_kind(a: &Asked, b: &Asked) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

impl Questions {
    /// The questions open, in the order they were asked: each its number, what it asks and
    /// whether it goes in front.
    pub fn open(&self) -> impl Iterator<Item = (u32, &Asked, bool)> {
        self.open.iter().map(|o| (o.id, &o.asked, o.at_once))
    }

    /// Whether a question like `asked` is open.
    fn has(&self, asked: &Asked) -> bool {
        self.open.iter().any(|o| same_kind(&o.asked, asked))
    }

    /// Ask `asked`; its number.
    fn ask(&mut self, asked: Asked, at_once: bool) -> u32 {
        self.last = self.last.wrapping_add(1).max(1);
        let id = self.last;
        self.open.push(Open { id, asked, at_once });
        while self.open.len() > MOST {
            let at = self
                .open
                .iter()
                .position(|o| matches!(o.asked, Asked::Message { .. }))
                .unwrap_or(0);
            self.open.remove(at);
        }
        id
    }

    /// Every question open that `which` picks taken down; whether any was.
    fn take_down(&mut self, which: impl Fn(&Asked) -> bool) -> bool {
        let before = self.open.len();
        self.open.retain(|o| !which(&o.asked));
        self.open.len() != before
    }

    /// Every question taken down.
    pub fn clear(&mut self) {
        self.open.clear();
    }

    /// A notice of the player's scripts heard (`player` the player's id, once named): the
    /// questions it asks, and those it takes down.
    // D2-01911, D2-01912, D2-01908, D2-01919, D2-02745
    // D2-02748, D2-02767, D2-02775, D2-02165, D2-02164
    pub fn hear(&mut self, n: &Notice, player: Option<u64>) {
        let death = |a: &Asked| matches!(a, Asked::Died | Asked::Resurrection { .. });
        let about_player = n.about.is_none() || n.about == player;
        match &n.said {
            Said::Player(PlayerNotice::Death { dead }) if about_player => {
                self.take_down(death);
                if *dead {
                    self.ask(Asked::Died, true);
                }
            }
            // An offer with no name reads as no offer.
            Said::Dialog(DialogNotice::ResurrectionOffered { by }) => {
                self.take_down(death);
                let asked = match by {
                    Some(by) => Asked::Resurrection {
                        by: Some(by.clone()),
                    },
                    None => Asked::Died,
                };
                self.ask(asked, true);
            }
            Said::Dialog(DialogNotice::EffectQuestion {
                target,
                source,
                effect,
                question,
            }) => {
                self.take_down(|a| matches!(a, Asked::Effect { .. }));
                self.ask(
                    Asked::Effect {
                        target: *target,
                        source: *source,
                        effect: *effect,
                        question: question.clone(),
                    },
                    true,
                );
            }
            Said::Dialog(DialogNotice::Message { text, at_once }) => {
                self.ask(Asked::Message { text: text.clone() }, *at_once);
            }
            Said::Fellowship(FellowshipNotice::Offered {
                leader,
                leader_name,
                fellowship,
            }) => {
                let asked = Asked::Fellowship {
                    leader: *leader,
                    leader_name: leader_name.clone(),
                    fellowship: fellowship.clone(),
                };
                if !self.has(&asked) {
                    self.ask(asked, false);
                }
            }
            Said::Fellowship(FellowshipNotice::OfferExpired { .. }) => {
                self.take_down(|a| matches!(a, Asked::Fellowship { .. }));
            }
            Said::Allegiance(AllegianceNotice::OathOffered { vassal, name }) => {
                let asked = Asked::Oath {
                    vassal: *vassal,
                    name: name.clone(),
                };
                if !self.has(&asked) {
                    self.ask(asked, false);
                }
            }
            Said::Allegiance(AllegianceNotice::OathAnswered { .. }) => {
                self.take_down(|a| matches!(a, Asked::Oath { .. }));
            }
            // Asked by the partner: the question whether to trade.
            Said::Trade(TradeNotice::Registered {
                asker,
                partner: name,
                ..
            }) if Some(*asker) != player => {
                let asked = Asked::Trade {
                    partner: *asker,
                    name: name.clone(),
                };
                if !self.has(&asked) {
                    self.ask(asked, false);
                }
            }
            Said::Trade(TradeNotice::Closed { .. }) => {
                self.take_down(|a| matches!(a, Asked::Trade { .. }));
            }
            _ => {}
        }
    }

    /// What answering question `id` (`yes` or no) asks of the game: its action, when it asks one.
    /// The question goes when that action is carried out ([`Questions::acted`]); one whose answer
    /// asks nothing goes now. A question not open asks nothing.
    // D2-01913, D2-01914, D2-01927, D2-02746, D2-02773, D2-02160
    pub fn answer(&mut self, id: u32, yes: bool) -> Option<Action> {
        let at = self.open.iter().position(|o| o.id == id)?;
        let action = match &self.open[at].asked {
            Asked::Died => Some(Action::Resurrect),
            Asked::Resurrection { .. } => Some(if yes {
                Action::Resurrect
            } else {
                Action::DeclineResurrection
            }),
            Asked::Effect {
                target,
                source,
                effect,
                ..
            } => yes.then_some(Action::ApplyTotem {
                target: *target,
                source: *source,
                effect: *effect,
            }),
            Asked::Message { .. } => None,
            Asked::Fellowship { .. } => Some(Action::Social(fellowship_answer(yes))),
            Asked::Oath { .. } => Some(Action::Social(oath_answer(yes))),
            Asked::Trade { partner, .. } => Some(Action::Trade(if yes {
                TradeAction::Open(*partner)
            } else {
                TradeAction::Close
            })),
            _ => None,
        };
        if action.is_none() {
            self.open.remove(at);
        }
        action
    }

    /// An action carried out: the questions it answers taken down (an offer to bring the player
    /// back turned down asks death's question again), and every question when the player logs off.
    pub fn acted(&mut self, action: &Action) {
        match action {
            Action::Resurrect => {
                self.take_down(|a| matches!(a, Asked::Died | Asked::Resurrection { .. }));
            }
            Action::DeclineResurrection => {
                if self.take_down(|a| matches!(a, Asked::Resurrection { .. })) {
                    self.ask(Asked::Died, true);
                }
            }
            Action::ApplyTotem { .. } => {
                self.take_down(|a| matches!(a, Asked::Effect { .. }));
            }
            Action::Social(
                SocialAction::AcceptRecruitment | SocialAction::DeclineRecruitment(_),
            ) => {
                self.take_down(|a| matches!(a, Asked::Fellowship { .. }));
            }
            Action::Social(SocialAction::TakeOath | SocialAction::RefuseOath(_)) => {
                self.take_down(|a| matches!(a, Asked::Oath { .. }));
            }
            Action::Trade(TradeAction::Open(_) | TradeAction::Close) => {
                self.take_down(|a| matches!(a, Asked::Trade { .. }));
            }
            Action::LogOff => self.clear(),
            _ => {}
        }
    }
}

/// The string tables the questions' words are in, by enumeration value, and the mapping that
/// finds a table's record by it.
const MESSAGES: u32 = 0x4100_0018;
const RESURRECTION_MESSAGES: u32 = 0x4100_000A;
const TABLE_MAPPING: u32 = 0x4000_0004;

/// A string of a table, by its name, with its variables, each a name and a string reference.
fn entry(table: u32, name: &str, variables: Vec<(&str, Option<TextRef>)>) -> TextRef {
    let token = auberean_assets::strings::token;
    TextRef {
        string: token(name),
        table,
        literal: None,
        variables: variables
            .into_iter()
            .map(|(id, value)| TextVariable {
                id: token(id),
                value: TextValue::Text(Box::new(value.unwrap_or_default())),
            })
            .collect(),
    }
}

impl Client {
    /// The questions open, each with its words as the game's own interface writes them.
    pub(crate) fn questions_view(&self) -> Vec<Question> {
        let cache = self.download.cache();
        let table = |value: u32| {
            if cache.is_empty() {
                0
            } else {
                cache.database().data_id_by_enum(value, TABLE_MAPPING)
            }
        };
        self.views
            .questions
            .open()
            .map(|(id, asked, at_once)| {
                // D2-01906, D2-01907, D2-02745, D2-02767, D2-02164
                let text = match asked {
                    Asked::Died => entry(table(MESSAGES), "ID_Death_YouDied", Vec::new()),
                    Asked::Resurrection { by } => entry(
                        table(RESURRECTION_MESSAGES),
                        "ID_ResurrectionPrompt",
                        vec![("PLAYER", by.clone())],
                    ),
                    Asked::Effect { question, .. } => question.clone().unwrap_or_default(),
                    Asked::Message { text } => text.clone().unwrap_or_default(),
                    Asked::Fellowship {
                        leader_name,
                        fellowship,
                        ..
                    } => entry(
                        table(MESSAGES),
                        "ID_Fellowship_RecruitText",
                        vec![
                            ("leader", leader_name.clone()),
                            ("name", fellowship.clone()),
                        ],
                    ),
                    Asked::Oath { name, .. } => entry(
                        table(MESSAGES),
                        "ID_Allegiance_SwearText",
                        vec![("value", name.clone())],
                    ),
                    Asked::Trade { name, .. } => entry(
                        table(MESSAGES),
                        "ID_SecureTrade_AcceptTradeText",
                        vec![("value", name.clone())],
                    ),
                    _ => TextRef::default(),
                };
                Question {
                    id,
                    asked: asked.clone(),
                    text,
                    at_once,
                }
            })
            .collect()
    }
}
