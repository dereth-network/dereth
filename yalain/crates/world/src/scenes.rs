//! The story's scenes: a story shard bestowing its scene and taking the player on, the scenes
//! every new character holds, the act registry as a player enters.
//!
//! **A story shard used** (walked to as a portal): the shard flashes and the player plays the
//! shard's behavior, the camera on the shard. Ten seconds later, in one tick: the scene added to
//! the player's story (when it is new), the player frozen, protected by the shard's magic, played
//! the scene (the whole act when the scene completes the act's seven), told it is teleporting and
//! taken into portal space; it comes out at the shard's destination two and a half seconds later.
//! When the client says the scenes are done, the protection ends three and a half seconds later:
//! the text, the player freed, the effect gone, its glows stopped.
//!
//! **A new character** holds the story's starting scenes, each added as it first enters.
//! **As a player enters** its session start carries the scenes it holds, by act.

use auberean_protocol::physics::{Fx, ObjectEvent, Word};
use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::native_object::{NativeObject, TextInfo};
use auberean_script::story::{Keyframe, SceneInfo};
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::calls::Value;
use crate::effects::{add_call, next_eid, remove_call};
use crate::graph::Builder;
use crate::places::{self, Trip, Use};
use crate::state::EffectState;
use crate::{progress, session, Clock, Server, Timer};

/// The story's calls.
pub mod function {
    /// The server's: scenes to play, a scene added to the player's story.
    pub const PLAY_SCENES: u32 = 0x8213_0021;
    pub const STORY_QUEST: u32 = 0x8213_0023;
    /// The client's: the scenes are done.
    pub const DONE: u32 = 0x8213_0013;
}

/// The native type of a list of scenes.
pub const SCENE_LIST: u16 = 0xA4;

/// The shard's behavior on the player (with the camera on the shard), its flash, the player's
/// glows in portal space.
pub mod look {
    pub const SHARD_BEHAVIOR: u32 = 0x4100_00A5;
    /// The behavior, its flag word (0x800), the camera, the context.
    pub const BEHAVIOR_BITS: u32 = auberean_protocol::types::BehaviorParams::BEHAVIOR
        | 0x800
        | auberean_protocol::types::BehaviorParams::CAMERA
        | auberean_protocol::types::BehaviorParams::CONTEXT;
    pub const FLASH: u32 = 0x4100_0044;
    pub const GLOW: u32 = 0x4100_000F;
    pub const SHIMMER: u32 = 0x8100_028E;
}

/// How long the shard's behavior plays before the scene, and the trip after it, in seconds.
pub const SHARD_WAIT: f64 = 10.1;
pub const IN_PORTAL_SPACE: f64 = 2.466;
/// How long after the scenes are done the shard's protection ends.
pub const PROTECTION_LINGERS: f64 = 3.39;

/// An act is whole with this many scenes held (hidden ones aside).
pub const ACT_WHOLE: usize = 7;

/// The shard's protection: its effect, its record's flags and categories, its end's text.
pub mod protection {
    pub const STATE: u32 = 0x7000_0976;
    pub const FLAGS: u32 = 81;
    pub const CATEGORIES: u32 = 1;
    pub const ENDED: (u32, u32) = (0xF271_1927, 0x2500_000B);
}

/// A scene as the client plays it.
#[must_use]
pub fn scene_info(sc: &yalain_content::story::Scene) -> SceneInfo {
    let text = |(id, table): (u32, u32)| TextInfo {
        string_id: id,
        table,
        ..TextInfo::default()
    };
    SceneInfo {
        id: sc.id,
        name: text(sc.name),
        act: sc.act,
        number: sc.number,
        hidden: u32::from(sc.hidden),
        playable: u32::from(sc.playable),
        keyframes: sc
            .keyframes
            .iter()
            .zip(1u32..)
            .map(|(k, n)| Keyframe {
                scene: sc.id,
                number: n,
                duration: k.seconds.to_bits(),
                video: k.picture,
                audio: k.music,
                voice: k.voice,
                subtitle: k.subtitle.map(text).unwrap_or_default(),
                movie: Vec::new(),
                tracks: k.tracks,
            })
            .collect(),
    }
}

/// Scenes to play.
#[must_use]
pub fn play_scenes(scenes: Vec<SceneInfo>) -> ServerMessage {
    let mut b = Builder::new();
    b.native(SCENE_LIST, NativeObject::Scenes(scenes));
    session::call(function::PLAY_SCENES, &[Arg::Object(b.graph())]).expect("scenes pack")
}

/// A scene added to the player's story.
#[must_use]
pub fn story_quest(scene: u32) -> ServerMessage {
    session::call(
        function::STORY_QUEST,
        &[Arg::Int(0), Arg::Int(1), Arg::Int(scene)],
    )
    .expect("words pack")
}

/// The act registry a session start carries: each act the player holds scenes of, with them.
#[must_use]
pub fn registry<S: Store>(s: &Server<S>, held: &[u32]) -> Vec<(u32, Vec<u32>)> {
    let mut acts: Vec<(u32, Vec<u32>)> = Vec::new();
    for id in held {
        let Some(sc) = s.story.scenes.get(id) else {
            continue;
        };
        if sc.hidden {
            continue;
        }
        match acts.iter_mut().find(|(a, _)| *a == sc.act) {
            Some((_, v)) => v.push(*id),
            None => acts.push((sc.act, vec![*id])),
        }
    }
    acts
}

/// A new character's first entry: each starting scene is added to its story.
pub fn first_entry<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let start = s.story.at_start.clone();
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    for id in start {
        if !p.play.scenes.contains(&id) {
            p.play.scenes.push(id);
            progress::private(net, world, &story_quest(id));
        }
    }
}

/// A story shard used, the player at it: it flashes, the player plays its behavior with the
/// camera on it; the scene follows ten seconds later.
pub fn shard_used<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    to: StoredPosition,
    scene: u32,
    clock: Clock,
) {
    let shard = u.item;
    let flash = ServerMessage::DoFx(ObjectEvent {
        sender: places::object_stamp(s, shard),
        action: Fx {
            fx: look::FLASH,
            scalar: 1.0,
        },
    });
    for (conn, q) in &s.players {
        if q.known.contains_key(&shard) {
            net.send(
                *conn,
                auberean_protocol::queue::EVENT,
                BlobKind::VISUAL,
                flash.encode(),
            );
        }
    }
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let mut b = BehaviorParams {
        bits: look::BEHAVIOR_BITS,
        behavior: look::SHARD_BEHAVIOR,
        camera_target: shard,
        camera_behavior: look::SHARD_BEHAVIOR,
        context,
        ..BehaviorParams::default()
    };
    b.words[8] = 1;
    let plays = ServerMessage::PlayBehavior(places::player_event(p, b));
    p.trip = Some(Trip {
        to,
        used: Some(u),
        step: None,
        resurrect: false,
        sent: false,
    });
    let id = p.character;
    places::to_all_knowing(s, net, id, &plays);
    s.after(
        clock,
        SHARD_WAIT,
        Timer::Trip {
            world,
            step: places::Step::Shard { scene },
        },
    );
}

/// The scenes a shard's scene plays: the whole act when it makes the act's seven, else itself.
fn to_play<S: Store>(s: &Server<S>, held: &[u32], scene: u32) -> Vec<SceneInfo> {
    let Some(sc) = s.story.scenes.get(&scene) else {
        return Vec::new();
    };
    let act = s.story.act(sc.act);
    let whole = act
        .iter()
        .filter(|x| !x.hidden && held.contains(&x.id))
        .count()
        >= ACT_WHOLE;
    if whole {
        act.into_iter().map(scene_info).collect()
    } else {
        vec![scene_info(sc)]
    }
}

/// The shard's wait is over: the scene added, the player frozen and protected, the scenes
/// played, into portal space.
pub fn bestow<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    scene: u32,
    clock: Clock,
) {
    let cast = net.game_time(clock.local);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if p.trip.is_none() {
        return;
    }
    let new = !p.play.scenes.contains(&scene);
    if new {
        p.play.scenes.push(scene);
    }
    let eid = next_eid(p);
    p.shard_protection = Some(eid);
    let id = p.character;
    let held = p.play.scenes.clone();
    if new {
        progress::private(net, world, &story_quest(scene));
    }
    progress::private(net, world, &session::movement_frozen(true));
    progress::private(net, world, &places::animation_frozen(true));
    #[allow(clippy::cast_possible_truncation)]
    let e = EffectState {
        eid,
        state: protection::STATE,
        cast,
        timeout: f32::MAX,
        caster: id,
        spellcraft: 0.0,
        pk: false,
        acting: 0,
        skill: 0,
        item: 0,
        flags: protection::FLAGS,
        categories: protection::CATEGORIES,
    };
    progress::private(net, world, &add_call(&e));
    let scenes = to_play(s, &held, scene);
    if !scenes.is_empty() {
        progress::private(net, world, &play_scenes(scenes));
    }
    places::shard_portal_space(s, net, world);
    s.after(
        clock,
        IN_PORTAL_SPACE,
        Timer::Trip {
            world,
            step: places::Step::Arrive,
        },
    );
}

/// The client's scenes are done: a shard's protection ends a few seconds later (the story page's
/// own plays are not answered).
pub fn done<S: Store>(s: &mut Server<S>, world: ConnId, v: &[Value], clock: Clock) {
    let _ = v;
    let Some(eid) = s.players.get(&world).and_then(|p| p.shard_protection) else {
        return;
    };
    s.after(
        clock,
        PROTECTION_LINGERS,
        Timer::ShardProtectionEnds { world, eid },
    );
}

/// The shard's protection ends: the text, the player freed, the effect gone, its glows stopped.
pub fn protection_ends<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, eid: u32) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if p.shard_protection != Some(eid) {
        return;
    }
    p.shard_protection = None;
    let ended = progress::text(protection::ENDED);
    progress::private(net, world, &crate::chat::show_text(8, ended));
    progress::private(net, world, &session::movement_frozen(false));
    progress::private(net, world, &places::animation_frozen(false));
    progress::private(net, world, &remove_call(eid));
    for fx in [look::SHIMMER, look::GLOW] {
        let Some(p) = s.players.get_mut(&world) else {
            return;
        };
        let stop = ServerMessage::StopFx(places::player_event(p, Word(fx)));
        let id = p.character;
        places::to_all_knowing(s, net, id, &stop);
    }
}
