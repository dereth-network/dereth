//! What the map window draws on and shows, from the region (the world map's picture, the land's
//! size, the closer map's table and its labels) and the calendar (the date and time); and the
//! social window's requests as the client's actions.

use auberean_assets::region::{BlockMap, DetailMap, MapNotes as NotesRecord};
use auberean_client_sdk::contract::SocialAction;
use auberean_client_sdk::Action;
use auberean_scene::WorldData;
use auberean_ui_game::map::{Atlas, Clock, DetailRow, DetailTable, MapNote, MapNotes};
use auberean_ui_game::social::Request;

/// The region's world map, the land's size, and its first table of detail maps (the region's
/// current one: the client never chooses another) with its block map and every label record its
/// pictures name.
// D2-07475, D2-07484
#[must_use]
pub fn atlas(world: &WorldData) -> Atlas {
    let region = &world.region.region;
    let (wide, long) = (
        i32::try_from(region.blocks_wide).unwrap_or(0),
        i32::try_from(region.blocks_long).unwrap_or(0),
    );
    let detail = region
        .detail_maps
        .first()
        .and_then(|t| world.records.get::<DetailMap>(t.record.0))
        .map(|d| detail_table(world, &d));
    Atlas {
        game_map: region.map.0,
        land: (wide << 3, long << 3),
        blocks: (wide, long),
        detail,
    }
}

fn detail_table(world: &WorldData, d: &DetailMap) -> DetailTable {
    let grid = world
        .records
        .get::<BlockMap>(d.block_map.0)
        .unwrap_or_default();
    let int = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
    let rows: Vec<DetailRow> = d
        .rows
        .iter()
        .map(|r| DetailRow {
            value: r.value,
            picture: r.picture.0,
            x: int(r.x),
            y: int(r.y),
            height: int(r.height),
            width: int(r.width),
            notes: r.notes.clone(),
        })
        .collect();
    let mut notes = Vec::new();
    for id in rows.iter().flat_map(|r| r.notes.iter().copied()) {
        if notes.iter().any(|(i, _)| *i == id) {
            continue;
        }
        if let Some(n) = world.records.get::<NotesRecord>(id) {
            notes.push((
                id,
                MapNotes {
                    strings: n.strings.0,
                    notes: n
                        .notes
                        .iter()
                        .map(|m| MapNote {
                            text: m.text,
                            icon: m.icon.0,
                            x: int(m.x),
                            y: int(m.y),
                        })
                        .collect(),
                },
            ));
        }
    }
    DetailTable {
        height: grid.height,
        width: grid.width,
        cells: grid.cells,
        rows,
        notes,
    }
}

/// The game's date and time at game time `t`, by the region's calendar.
#[must_use]
pub fn clock(world: &WorldData, t: f64) -> Option<Clock> {
    let cal = world.region.calendar.as_ref()?;
    let d = auberean_world::calendar::date(cal, t)?;
    Some(Clock {
        year: i32::try_from(d.year).unwrap_or(0),
        year_name: d.year_name,
        day: i32::try_from(d.day).unwrap_or(0),
        season: d.season_name,
        part: d.part_name,
    })
}

/// A social window's request as the client's action: the friends page's go to the friends
/// service.
#[must_use]
pub fn social_action(r: Request) -> Action {
    use SocialAction as S;
    Action::Social(match r {
        Request::CreateFellowship {
            name,
            social,
            looting,
        } => S::CreateFellowship {
            name: name.encode_utf16().collect(),
            social,
            looting,
        },
        Request::DisbandFellowship => S::DisbandFellowship,
        Request::LeaveFellowship => S::LeaveFellowship,
        Request::Recruit(id) => S::Recruit(id),
        Request::Dismiss(id) => S::Dismiss(id),
        Request::AcceptRecruitment => S::AcceptRecruitment,
        Request::DeclineRecruitment(why) => S::DeclineRecruitment(why),
        Request::QueryAllegiance(id) => S::QueryAllegiance(id),
        Request::Swear => S::Swear,
        Request::Break(id) => S::Break(id),
        Request::TakeOath => S::TakeOath,
        Request::RefuseOath(why) => S::RefuseOath(why),
        Request::Emote { string, table } => return Action::Emote { string, table },
        Request::AssistedTell(name) => return Action::AssistedTell { name },
        Request::AddFriend(name) => S::AddFriend(name.encode_utf16().collect()),
        Request::RemoveFriend(name) => S::RemoveFriend(name.encode_utf16().collect()),
        Request::GetFriends => S::GetFriends,
        Request::SetLocation { mode, location } => S::SetState {
            mode,
            location: location.encode_utf16().collect(),
        },
    })
}
