//! The story's scenes played over the play screen on the retail layouts.

use auberean_text::StringInfo;
use auberean_ui::draw;
use auberean_ui::types::key_id;
use auberean_ui::Token;
use auberean_ui_game::gameplay::{notice, Record, Request};
use auberean_ui_game::story::{Keyframe, Scene, Scenes, Sound};

use super::stage::{said, Lacking, World, LACKING};

const PICTURE_1: u32 = 0x0600_1001;
const PICTURE_2: u32 = 0x0600_1002;
const MUSIC: u32 = 0x0A00_0001;
const VOICE: u32 = 0x0A00_0002;

fn scenes() -> Scenes {
    let step = |picture, subtitle: &str, duration| Keyframe {
        picture: Some(picture),
        subtitle: Some(StringInfo::literal(subtitle)),
        duration,
        ..Keyframe::default()
    };
    let mut first = step(PICTURE_1, "Long ago", Some(2.0));
    first.music = Some(MUSIC);
    first.voice = Some(VOICE);
    Scenes {
        scenes: vec![
            Scene {
                id: 1,
                playable: true,
                keyframes: vec![first, step(PICTURE_2, "The war began", None)],
            },
            Scene {
                id: 2,
                playable: false,
                keyframes: vec![step(PICTURE_1, "Not shown", None)],
            },
        ],
        packed: vec![1, 2, 3, 4],
    }
}

fn sounds(out: &[Request]) -> Vec<Sound> {
    out.iter()
        .filter_map(|r| match r {
            Request::StorySound(s) => Some(*s),
            _ => None,
        })
        .collect()
}

/// The scripts' scenes put the story up over the screen; once its background has faded in, the
/// world hides and the playable scenes' steps play one after another: each step's picture in the
/// frame whose turn it is, its music and voice, its subtitle; a step lasts its own time, else five
/// seconds. After the last, the music and voice stop, the world shows and the scripts hear which
/// scenes were played. Escape cuts a story short; a scene on the way through a portal waits for
/// the player to arrive.
///
/// Behaviour: ui.story.the-storys-scenes-play-over-the-screen
#[test]
fn the_storys_scenes_play_over_the_screen() {
    let mut w = World::new();
    let record = || vec![Some(Record::Scenes(scenes()))];
    w.tick(&[said(notice::PLAY_SEQUENCE, vec![0], record())]);
    assert!(w.play.story().is_some(), "the story up");
    let mut out = Vec::new();
    for _ in 0..200 {
        out.extend(w.tick(&[]));
        if w.play
            .story()
            .is_some_and(auberean_ui_game::story::Story::playing)
        {
            break;
        }
    }
    assert!(out.contains(&Request::ShowWorld(false)), "the world hidden");
    assert_eq!(
        sounds(&out),
        vec![Sound::Music(Some(MUSIC)), Sound::Voice(Some(VOICE))]
    );
    // The first picture turns the frames: it shows in the second.
    let (one, two) = (w.el("StoryField1"), w.el("StoryField2"));
    assert_eq!(w.text(w.under(two, "TextField")), "Long ago");
    assert_eq!(w.state(two), Token::of("Frame2"));
    assert_eq!(
        w.play.story().map(auberean_ui_game::story::Story::step),
        Some((1, 2)),
        "two steps of the playable scene"
    );
    // The first lasts two seconds: then the second, in the first frame.
    w.wait(2.1);
    assert_eq!(
        w.play.story().map(auberean_ui_game::story::Story::step),
        Some((2, 2))
    );
    assert_eq!(w.state(one), Token::of("Frame1"));
    assert_eq!(w.state(two), Token::of("Frame1"));
    assert_eq!(w.text(w.under(one, "TextField")), "The war began");
    // Five seconds, then done.
    let out = w.wait(5.1);
    assert!(w.play.story().is_none(), "gone");
    assert!(out.contains(&Request::ShowWorld(true)));
    assert!(
        out.contains(&Request::PlayScenesDone(vec![1, 2, 3, 4])),
        "{out:?}"
    );

    // Escape cuts the next one short.
    w.tick(&[said(notice::PLAY_SEQUENCE, vec![0], record())]);
    let _ = w.wait(3.0);
    w.ui.key(key_id::ESCAPE, true);
    let mut out = w.tick(&[]);
    w.ui.key(key_id::ESCAPE, false);
    out.extend(w.tick(&[]));
    assert!(w.play.story().is_none(), "cut short");
    assert!(out.contains(&Request::PlayScenesDone(vec![1, 2, 3, 4])));

    // On the way through a portal: Escape does nothing, and finished it waits for the player.
    w.tick(&[said(notice::PORTAL_SCENE, vec![1, 0], record())]);
    let _ = w.wait(3.0);
    w.ui.key(key_id::ESCAPE, true);
    w.tick(&[]);
    w.ui.key(key_id::ESCAPE, false);
    w.tick(&[]);
    assert!(w.play.story().is_some(), "not cut short");
    let out = w.wait(10.0);
    assert!(!out.iter().any(|r| matches!(r, Request::PlayScenesDone(_))));
    let mut out = Vec::new();
    w.play.player_ready_to_enter_world(&mut w.ui, &mut out);
    assert!(w.play.story().is_none(), "the player arrived");
    assert!(out.contains(&Request::PlayScenesDone(Vec::new())));
}

/// The story's background stands at the shortcut strip's depth and came after it, so the strip is
/// drawn in front of it, as the only part of the play screen that shows through, and the pointer
/// meets it there.
///
/// Behaviour: ui.story.the-shortcut-strip-shows-in-front-of-the-storys-background
#[test]
fn the_shortcut_strip_shows_in_front_of_the_storys_background() {
    let mut w = World::new();
    let record = || vec![Some(Record::Scenes(scenes()))];
    let strip = w.ui.element(Token::of("ShortcutListBox")).unwrap();
    let area = w.ui.screen_rect(strip).unwrap();
    w.tick(&[said(notice::PLAY_SEQUENCE, vec![0], record())]);
    for _ in 0..200 {
        w.tick(&[]);
        if w.play
            .story()
            .is_some_and(auberean_ui_game::story::Story::playing)
        {
            break;
        }
    }
    let mut screen = draw::Surface::new(1024, 768);
    draw::draw(&w.ui, &mut screen);
    let lit = (area.x0..=area.x1)
        .any(|x| (area.y0..=area.y1).any(|y| screen.get(x, y)[..3].iter().any(|c| *c != 0)));
    assert!(lit, "the strip shows over the black");
    let below =
        w.ui.hit(area.x0 + 5, area.y0 + 5)
            .and_then(|h| w.ui.node(h));
    assert_ne!(
        below.map(|n| n.id),
        Some(Token::of("BackgroundField")),
        "the pointer meets the strip"
    );
    assert!(below.is_some());
    assert!(
        !(0..100).any(|x| (300..400).any(|y| screen.get(x, y)[..3].iter().any(|c| *c != 0))),
        "the rest of the screen is the background's black"
    );
}

/// A step's subtitle naming a string table the data lacks is given to the frame's text as a
/// reference: the text waits until the table has come, then shows the subtitle.
///
/// Behaviour: ui.story.a-subtitle-waits-for-its-string-table
#[test]
fn a_subtitle_waits_for_its_string_table_then_shows() {
    let lacking = Lacking::new();
    let mut w = World::over(lacking.clone());
    let mut lone = scenes();
    lone.scenes.truncate(1);
    lone.scenes[0].keyframes.truncate(1);
    lone.scenes[0].keyframes[0].subtitle = Some(StringInfo::entry(LACKING, 7));
    w.tick(&[said(
        notice::PLAY_SEQUENCE,
        vec![0],
        vec![Some(Record::Scenes(lone))],
    )]);
    for _ in 0..200 {
        w.tick(&[]);
        if w.play
            .story()
            .is_some_and(auberean_ui_game::story::Story::playing)
        {
            break;
        }
    }
    let field = w.under(w.el("StoryField2"), "TextField");
    assert_eq!(w.text(field), "", "it waits for the table");
    lacking.arrive();
    w.tick(&[]);
    assert_eq!(w.text(field), "From the server\n");
}
