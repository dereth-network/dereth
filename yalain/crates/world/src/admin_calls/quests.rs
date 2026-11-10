//! An admin's commands over a player's quests and story.

use yalain_content::quests::Update;
use yalain_store::Store;

use super::{Args, Cx};
use crate::quests::{status, update};

/// The calls, by their function ids.
pub mod function {
    pub const LIST: u32 = 0x8201_001E;
    pub const REMOVE: u32 = 0x8201_0019;
    pub const RETRY: u32 = 0x8201_0116;
    /// `RequestQuestUpdate`: a quest, and its kind of update.
    pub const UPDATE: u32 = 0x8201_0077;
    pub const STORY_LIST: u32 = 0x8201_004B;
    pub const PLAY_SCENE: u32 = 0x8201_006E;
    pub const STORY_UPDATE: u32 = 0x8201_0060;
}

/// A call of this kind: its answer; `None` for any other.
pub(super) fn call<S: Store>(cx: &mut Cx<'_, S>, f: u32, a: &Args<'_>) -> Option<Vec<String>> {
    Some(match f {
        function::LIST => list(cx, a),
        function::REMOVE | function::RETRY => remove(cx, a, f == function::RETRY),
        function::UPDATE => update(cx, a),
        function::STORY_LIST => story(cx, a),
        function::PLAY_SCENE => play_scene(cx, a),
        function::STORY_UPDATE => story_update(cx, a),
        _ => return None,
    })
}

fn list<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get(&w) else {
        return Vec::new();
    };
    let mut lines = vec![format!("{} holds {} quests.", p.name, p.play.quests.len())];
    lines.extend(p.play.quests.iter().map(|st| {
        format!(
            "{:08X}: status {:#x}, phase {}, taken {}, updated {}",
            st.quest, st.status, st.phase, st.started, st.updated
        )
    }));
    lines
}

/// `/qst remove` takes a quest off a player; `/qst retry` too, so it may be taken again at once.
fn remove<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>, retry: bool) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let quest = a.int(1).unwrap_or(0);
    let name = cx.name_of(w);
    if !crate::quests::remove(cx.s, cx.net, w, quest, cx.clock) {
        return vec![format!("{name} does not hold quest {quest:08X}.")];
    }
    crate::world::keep_player(cx.s, w);
    vec![format!(
        "{name} no longer holds quest {quest:08X}{}.",
        if retry { ", and may take it again" } else { "" }
    )]
}

/// `/qst bestow`, `advance`, `complete` and `fail` move a player's quest as its kind of update says,
/// as the world's own steps move it: given anew, on to a phase (the next, with none named), done or
/// failed; or taken off, as `/qst remove` takes it. A quest the world doesn't define, one not under
/// way that only one under way may take, and a kind of update with no counterpart here are answered
/// so.
fn update<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let (quest, kind) = (a.int(1).unwrap_or(0), a.int(2).unwrap_or(0));
    if cx.s.quests.quest(quest).is_none() {
        return vec![format!("The world defines no quest {quest:08X}.")];
    }
    let name = cx.name_of(w);
    let under_way = cx.s.players.get(&w).and_then(|p| {
        p.play
            .quests
            .iter()
            .find(|q| q.quest == quest && q.status == status::UNDERWAY)
            .copied()
    });
    let not_under_way = || vec![format!("{name} does not have quest {quest:08X} under way.")];
    let phase = kind & 0x0FFF_FFFF;
    let up = match kind & 0xF000_0000 {
        update::BESTOW => Update::Bestow,
        update::ADVANCE => match under_way {
            Some(q) if phase == 0 => Update::Advance(q.phase + 1),
            Some(_) => Update::Advance(phase),
            None => return not_under_way(),
        },
        update::COMPLETE => Update::Complete,
        update::FAIL if under_way.is_none() => return not_under_way(),
        update::FAIL => Update::Fail,
        update::REMOVE => return remove(cx, a, false),
        _ => return vec![format!("This server has no quest update {kind:#010x}.")],
    };
    crate::quests::apply(cx.s, cx.net, w, quest, up, 0, cx.clock);
    let now = match up {
        Update::Advance(n) => format!("at phase {n}"),
        Update::Complete => "done".to_owned(),
        Update::Fail => "failed".to_owned(),
        Update::Bestow | Update::Nothing => "given".to_owned(),
    };
    vec![format!("{name}'s quest {quest:08X} is {now}.")]
}

fn story<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get(&w) else {
        return Vec::new();
    };
    vec![format!(
        "{} holds the scenes {:08X?}.",
        p.name, p.play.scenes
    )]
}

fn play_scene<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let scene = a.int(1).unwrap_or(0);
    let Some(sc) = cx.s.story.scenes.get(&scene).cloned() else {
        return vec![format!("The world has no scene {scene:08X}.")];
    };
    let m = crate::scenes::play_scenes(vec![crate::scenes::scene_info(&sc)]);
    crate::progress::private(cx.net, w, &m);
    vec![format!("{} is shown scene {scene:08X}.", cx.name_of(w))]
}

fn story_update<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let (scene, add) = (a.int(1).unwrap_or(0), a.flag(2).unwrap_or(true));
    let Some(p) = cx.s.players.get_mut(&w) else {
        return Vec::new();
    };
    if add {
        if !p.play.scenes.contains(&scene) {
            p.play.scenes.push(scene);
        }
    } else {
        p.play.scenes.retain(|x| *x != scene);
    }
    let name = p.name.clone();
    if add {
        let m = crate::scenes::story_quest(scene);
        crate::progress::private(cx.net, w, &m);
    }
    crate::world::keep_player(cx.s, w);
    vec![format!(
        "{name} {} scene {scene:08X}.",
        if add { "holds" } else { "no longer holds" }
    )]
}
