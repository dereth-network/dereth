//! The game's questions to the player, kept for every front end: which notices ask one, what an
//! answer does, what takes one down, and the questions as the view carries them.

use auberean_client_contract::notice::{
    AllegianceNotice, DialogNotice, FellowshipNotice, Notice, PlayerNotice, Said, TradeNotice,
};
use auberean_client_contract::question::Asked;
use auberean_client_contract::text::{TextRef, TextValue};
use auberean_client_contract::{Action, SocialAction, TradeAction};
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::host::PanelNotice;
use auberean_client_runtime::questions::{Questions, FELLOWSHIP_DECLINED, MOST, OATH_DECLINED};
use auberean_client_runtime::said::native;
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_script::native_object::NativeObject;

use super::download::file;

const PLAYER: u64 = 0x2130_0000_0000_00A1;
const OTHER: u64 = 0x2130_0000_0000_00B2;

fn said(said: Said) -> Notice {
    Notice { about: None, said }
}

/// The questions open: what each asks.
fn asked(q: &Questions) -> Vec<Asked> {
    q.open().map(|(_, a, _)| a.clone()).collect()
}

/// The number of the open question that `which` picks.
fn id_of(q: &Questions, which: impl Fn(&Asked) -> bool) -> u32 {
    q.open()
        .find(|(_, a, _)| which(a))
        .map(|(id, _, _)| id)
        .expect("the question open")
}

fn offered() -> Notice {
    said(Said::Fellowship(FellowshipNotice::Offered {
        leader: OTHER,
        leader_name: Some(TextRef::literal("Cara")),
        fellowship: Some(TextRef::literal("The Hunt")),
    }))
}

/// Death asks its one question, which brings the player back; an offer to bring the player back
/// takes its place, its yes brings the player back and its no turns the offer down and asks
/// death's question again (an offer with no name is death's own question); living again takes
/// both down. A notice about another object asks nothing.
///
/// Behaviour: ui.questions.death-and-an-offer-to-bring-the-player-back
#[test]
fn death_asks_to_be_brought_back_and_an_offer_takes_its_place() {
    let mut q = Questions::default();
    let dead = |dead| said(Said::Player(PlayerNotice::Death { dead }));
    q.hear(
        &Notice {
            about: Some(OTHER),
            ..dead(true)
        },
        Some(PLAYER),
    );
    assert!(asked(&q).is_empty(), "another's death asks nothing");
    q.hear(&dead(true), Some(PLAYER));
    assert_eq!(asked(&q), vec![Asked::Died]);
    assert!(q.open().all(|(_, _, first)| first), "death's goes in front");
    let died = id_of(&q, |a| *a == Asked::Died);
    assert_eq!(q.answer(died, false), Some(Action::Resurrect));

    let by = Some(TextRef::literal("Cara"));
    q.hear(
        &said(Said::Dialog(DialogNotice::ResurrectionOffered {
            by: by.clone(),
        })),
        Some(PLAYER),
    );
    assert_eq!(asked(&q), vec![Asked::Resurrection { by: by.clone() }]);
    let offer = id_of(&q, |a| matches!(a, Asked::Resurrection { .. }));
    assert_eq!(q.answer(offer, true), Some(Action::Resurrect));
    let no = q.answer(offer, false);
    assert_eq!(no, Some(Action::DeclineResurrection));
    q.acted(&Action::DeclineResurrection);
    assert_eq!(asked(&q), vec![Asked::Died], "turned down: asked again");

    q.hear(
        &said(Said::Dialog(DialogNotice::ResurrectionOffered { by: None })),
        Some(PLAYER),
    );
    assert_eq!(asked(&q), vec![Asked::Died], "no name: no offer");
    q.acted(&Action::Resurrect);
    assert!(asked(&q).is_empty(), "brought back: gone");

    q.hear(&dead(true), Some(PLAYER));
    q.hear(&dead(false), Some(PLAYER));
    assert!(asked(&q).is_empty(), "alive again: gone");
}

/// A fellowship's place offered is asked once while one is open: yes takes the place, no turns it
/// down with the declining reason; the offer running out takes it down. An oath offered likewise
/// (taken or refused), taken down when the oath was done. A trade the partner asks for is asked
/// (one the player asked for is not): yes opens it with the partner, no closes it; the trade
/// closing takes it down.
///
/// Behaviour: ui.questions.offers-of-a-place-an-oath-and-a-trade
#[test]
fn offers_of_a_place_an_oath_and_a_trade_are_asked_and_answered() {
    let mut q = Questions::default();
    q.hear(&offered(), Some(PLAYER));
    q.hear(&offered(), Some(PLAYER));
    assert_eq!(asked(&q).len(), 1, "one at a time");
    let place = id_of(&q, |a| matches!(a, Asked::Fellowship { .. }));
    assert_eq!(
        q.answer(place, true),
        Some(Action::Social(SocialAction::AcceptRecruitment))
    );
    assert_eq!(
        q.answer(place, false),
        Some(Action::Social(SocialAction::DeclineRecruitment(
            FELLOWSHIP_DECLINED
        )))
    );
    q.acted(&Action::Social(SocialAction::AcceptRecruitment));
    assert!(asked(&q).is_empty(), "the place taken: gone");
    q.hear(&offered(), Some(PLAYER));
    q.hear(
        &said(Said::Fellowship(FellowshipNotice::OfferExpired {
            leader: OTHER,
        })),
        Some(PLAYER),
    );
    assert!(asked(&q).is_empty(), "the offer ran out: gone");

    let oath = said(Said::Allegiance(AllegianceNotice::OathOffered {
        vassal: OTHER,
        name: Some(TextRef::literal("Cara")),
    }));
    q.hear(&oath, Some(PLAYER));
    let id = id_of(&q, |a| matches!(a, Asked::Oath { .. }));
    assert_eq!(
        q.answer(id, true),
        Some(Action::Social(SocialAction::TakeOath))
    );
    assert_eq!(
        q.answer(id, false),
        Some(Action::Social(SocialAction::RefuseOath(OATH_DECLINED)))
    );
    q.hear(
        &said(Said::Allegiance(AllegianceNotice::OathAnswered {
            patron: PLAYER,
            answer: 1,
        })),
        Some(PLAYER),
    );
    assert!(asked(&q).is_empty(), "the oath done: gone");

    let registered = |asker, asked| {
        said(Said::Trade(TradeNotice::Registered {
            asker,
            asked,
            partner: Some(TextRef::literal("Cara")),
        }))
    };
    q.hear(&registered(PLAYER, OTHER), Some(PLAYER));
    assert!(asked(&q).is_empty(), "the player's own ask: no question");
    q.hear(&registered(OTHER, PLAYER), Some(PLAYER));
    let trade = id_of(
        &q,
        |a| matches!(a, Asked::Trade { partner, .. } if *partner == OTHER),
    );
    assert_eq!(
        q.answer(trade, true),
        Some(Action::Trade(TradeAction::Open(OTHER)))
    );
    assert_eq!(
        q.answer(trade, false),
        Some(Action::Trade(TradeAction::Close))
    );
    q.hear(
        &said(Said::Trade(TradeNotice::Closed {
            by: OTHER,
            reason: 0,
        })),
        Some(PLAYER),
    );
    assert!(asked(&q).is_empty(), "the trade closed: gone");
}

/// An effect's question takes the place of the last one: yes lets the effect go on, no asks
/// nothing and takes it down. A message is read with its one answer, which asks nothing; it goes
/// in front when the scripts say so. Logging off takes every question down; past the most open,
/// the oldest message goes first.
///
/// Behaviour: ui.questions.an-effect-s-question-and-a-message
#[test]
fn an_effects_question_and_a_message_are_answered_and_logging_off_takes_all_down() {
    let mut q = Questions::default();
    let effect = |effect| {
        said(Said::Dialog(DialogNotice::EffectQuestion {
            target: PLAYER,
            source: OTHER,
            effect,
            question: Some(TextRef::literal("Apply it?")),
        }))
    };
    q.hear(&effect(5), Some(PLAYER));
    q.hear(&effect(6), Some(PLAYER));
    let effects = asked(&q);
    assert!(
        matches!(effects.as_slice(), [Asked::Effect { effect: 6, .. }]),
        "the new one in place of the last: {effects:?}"
    );
    let id = id_of(&q, |a| matches!(a, Asked::Effect { .. }));
    assert_eq!(
        q.answer(id, true),
        Some(Action::ApplyTotem {
            target: PLAYER,
            source: OTHER,
            effect: 6
        })
    );
    assert_eq!(q.answer(id, false), None);
    assert!(asked(&q).is_empty(), "no: gone, nothing asked");

    let message = |at_once| {
        said(Said::Dialog(DialogNotice::Message {
            text: Some(TextRef::literal("Welcome")),
            at_once,
        }))
    };
    q.hear(&message(false), Some(PLAYER));
    q.hear(&message(true), Some(PLAYER));
    let fronts: Vec<bool> = q.open().map(|(_, _, f)| f).collect();
    assert_eq!(fronts, vec![false, true]);
    let first = q.open().next().map(|(id, _, _)| id).expect("a message");
    assert_eq!(q.answer(first, true), None);
    assert_eq!(asked(&q).len(), 1, "read: gone");

    q.hear(&offered(), Some(PLAYER));
    for _ in 0..MOST {
        q.hear(&message(false), Some(PLAYER));
    }
    let open = asked(&q);
    assert_eq!(open.len(), MOST);
    assert!(
        open.iter().any(|a| matches!(a, Asked::Fellowship { .. })),
        "the messages go before the questions"
    );
    q.acted(&Action::LogOff);
    assert!(asked(&q).is_empty(), "logged off: all gone");
}

fn client() -> Client {
    Client::new(
        ClientSettings {
            login_server: "127.0.0.1:9000".parse().expect("an address"),
            account: b"a".to_vec(),
            ticket: Vec::new(),
            timestamp: 0x5000_0000,
            session_key: [0; 64],
            script_sends: false,
        },
        ClientData {
            files: DataFiles {
                files: vec![file(0x1_0000_0000, &[]), file(0x2_0000_0001, &[])],
                language: ENGLISH,
            },
            scripts: None,
        },
    )
}

/// A notice as the scripts send it to every panel: its words, and the text at a word's place.
fn sent(native: u32, words: Vec<u32>, texts: &[(usize, &str)]) -> PanelNotice {
    let n = words.len();
    let mut objects = vec![None; n];
    for (at, text) in texts {
        objects[*at] = Some(NativeObject::Text(text.encode_utf16().collect()));
    }
    PanelNotice {
        native,
        about: None,
        words,
        objects,
        requests: vec![None; n],
        effect_lines: vec![Vec::new(); n],
        listed: vec![Vec::new(); n],
        selections: vec![None; n],
    }
}

fn refresh(c: &mut Client) {
    c.refresh_view(&|_| Vec::new());
}

/// The client asks the questions the scripts' notices waiting to be taken ask, once each however
/// often the view is refreshed before they are taken, worded from the game's string tables (a
/// fellowship's place with the leader's name and the fellowship's); an answer through the
/// contract does what it asks and the question goes; a no to an offer to bring the player back
/// asks death's question again.
///
/// Behaviour: ui.questions.the-view-carries-the-open-questions
#[test]
fn the_view_carries_the_questions_the_notices_ask_until_they_are_answered() {
    let mut c = client();
    c.notices.push(sent(
        native::FELLOWSHIP_OFFERED,
        vec![0, 0xB2, 0, 0],
        &[(2, "Cara"), (3, "The Hunt")],
    ));
    c.notices
        .push(sent(native::MESSAGE, vec![0, 0], &[(0, "Welcome")]));
    refresh(&mut c);
    refresh(&mut c);
    let qs = c.view().questions.clone();
    assert_eq!(qs.len(), 2, "each heard once: {qs:?}");
    assert_eq!(
        qs[1].text,
        TextRef::literal("Welcome"),
        "a message's own words"
    );
    let q = &qs[0];
    assert!(!q.one_answer());
    assert_eq!(
        q.text.string,
        auberean_assets::strings::token("ID_Fellowship_RecruitText")
    );
    let named: Vec<(u32, TextValue)> = q
        .text
        .variables
        .iter()
        .map(|v| (v.id, v.value.clone()))
        .collect();
    let token = auberean_assets::strings::token;
    assert_eq!(
        named,
        vec![
            (
                token("leader"),
                TextValue::Text(Box::new(TextRef::literal("Cara")))
            ),
            (
                token("name"),
                TextValue::Text(Box::new(TextRef::literal("The Hunt")))
            ),
        ]
    );
    let _ = c.take_notices();
    c.act(Action::AnswerQuestion {
        question: q.id,
        yes: false,
    });
    c.act(Action::AnswerQuestion {
        question: qs[1].id,
        yes: true,
    });
    refresh(&mut c);
    assert!(c.view().questions.is_empty(), "answered: gone");

    c.notices
        .push(sent(native::RESURRECTION_OFFERED, vec![0], &[(0, "Cara")]));
    refresh(&mut c);
    let offer = c.view().questions[0].clone();
    assert!(matches!(offer.asked, Asked::Resurrection { .. }));
    let _ = c.take_notices();
    c.act(Action::AnswerQuestion {
        question: offer.id,
        yes: false,
    });
    refresh(&mut c);
    let again: Vec<Asked> = c.view().questions.iter().map(|q| q.asked.clone()).collect();
    assert_eq!(
        again,
        vec![Asked::Died],
        "turned down: death's question again"
    );
    assert!(c.view().questions[0].one_answer());
    c.act(Action::AnswerQuestion {
        question: c.view().questions[0].id,
        yes: true,
    });
    refresh(&mut c);
    assert!(c.view().questions.is_empty(), "brought back: gone");
}
