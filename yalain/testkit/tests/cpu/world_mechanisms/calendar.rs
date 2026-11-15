//! A quest whose time runs on the calendar: away from the world, its time runs on, where every
//! other quest's stands still.

use yalain_content::quests::QuestTable;
use yalain_world::quests::status;
use yalain_world::state::QuestState;

const IN_WORLD: u32 = 0x4000_0E31;
const BY_CALENDAR: u32 = 0x4000_0E32;

fn table() -> QuestTable {
    let timed = |id: u32, by_calendar: bool| yalain_content::quests::QuestDef {
        id,
        duration: 3600.0,
        duration_by_calendar: by_calendar,
        ..super::common::quest()
    };
    QuestTable {
        quests: vec![timed(IN_WORLD, false), timed(BY_CALENDAR, true)],
        ..QuestTable::default()
    }
}

fn under_way(quest: u32, started: i64) -> QuestState {
    QuestState {
        quest,
        status: status::UNDERWAY,
        phase: 1,
        started,
        updated: started,
    }
}

/// A character away 5,000 s enters at 10,000: its hour-long quest of time in the world, given at
/// 1,000, counts from 5,000 later and is still under way; the same quest on the calendar ran out at
/// 4,600, while it was away, and has failed as of then.
/// Behaviour: server.quests.duration-on-the-calendar
#[test]
fn a_quest_on_the_calendar_runs_out_while_its_player_is_away() {
    let mut held = [under_way(IN_WORLD, 1_000), under_way(BY_CALENDAR, 1_000)];
    yalain_world::quests::time_away(&table(), &mut held, 5_000, 10_000);
    assert_eq!((held[0].status, held[0].started), (status::UNDERWAY, 6_000));
    assert_eq!((held[1].status, held[1].updated), (status::FAILED, 4_600));
}

/// The quest on the calendar with time left when its player enters keeps counting from its giving.
/// Behaviour: server.quests.duration-on-the-calendar
#[test]
fn a_quest_on_the_calendar_with_time_left_counts_from_its_giving() {
    let mut held = [under_way(BY_CALENDAR, 9_000)];
    yalain_world::quests::time_away(&table(), &mut held, 500, 10_000);
    assert_eq!((held[0].status, held[0].started), (status::UNDERWAY, 9_000));
}
