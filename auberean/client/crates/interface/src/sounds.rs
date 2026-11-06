//! What the screens and panels ask to be heard, as the shell's sound requests: the sound playing
//! across the screens before the world, a story's music and voice, the panels' own sounds, and the
//! sounds the elements' media play.

use auberean_audio::{Looping, PlayRequest, PlayType};
use auberean_shell::interface::{Request, SoundOrder, SoundSlot};
use auberean_ui_game::story;

/// Seconds the screens and the story fade a sound out over when they stop it.
pub const FADE: f32 = 2.0;

/// A sound across the screens before the world (a screen's music, a song of the credits): kept to
/// be stopped when the next screen's begins or the world's play screen is made.
// D2-11867, D2-11868
#[must_use]
pub fn screens(id: u32) -> Request {
    Request::Sound(SoundOrder::Play {
        data: PlayRequest::new(id),
        slot: Some(SoundSlot::Screens),
    })
}

/// The sound across the screens stopped, fading out.
#[must_use]
pub fn screens_silence() -> Request {
    Request::Sound(SoundOrder::Stop {
        slot: SoundSlot::Screens,
        fade: FADE,
    })
}

/// A sound played once, kept by no one: a panel's alert, an element's sound.
#[must_use]
pub fn once(id: u32) -> Request {
    Request::Sound(SoundOrder::Play {
        data: PlayRequest::new(id),
        slot: None,
    })
}

/// What a story asks to be heard: its music (looping) or its voice played alone, each stopping
/// the one before, fading out; or each stopped, fading out.
#[must_use]
pub fn story(s: story::Sound) -> Vec<Request> {
    let (slot, id, looping) = match s {
        story::Sound::Music(id) => (SoundSlot::StoryMusic, id, Looping::ForceOn),
        story::Sound::Voice(id) => (SoundSlot::StoryVoice, id, Looping::Inherit),
    };
    let mut out = vec![Request::Sound(SoundOrder::Stop { slot, fade: FADE })];
    if let Some(id) = id {
        let mut data = PlayRequest::new(id);
        data.kind = PlayType::AllExclusive;
        data.looping = looping;
        out.push(Request::Sound(SoundOrder::Play {
            data,
            slot: Some(slot),
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the requests' shape; what each does is the sound manager's to claim)

    use super::*;

    #[test]
    fn a_storys_music_loops_alone_and_its_voice_plays_alone_each_stopping_the_last() {
        let music = story(story::Sound::Music(Some(0x2C00_0001)));
        let Request::Sound(SoundOrder::Stop { slot, fade }) = music[0] else {
            panic!("a stop first: {music:?}");
        };
        assert_eq!((slot, fade), (SoundSlot::StoryMusic, FADE));
        let Request::Sound(SoundOrder::Play { data, slot }) = music[1] else {
            panic!("then a play: {music:?}");
        };
        assert_eq!(slot, Some(SoundSlot::StoryMusic));
        assert_eq!(
            (data.kind, data.looping, data.channel),
            (
                PlayType::AllExclusive,
                Looping::ForceOn,
                auberean_audio::Channel::Main
            )
        );
        let voice = story(story::Sound::Voice(Some(0x2C00_0002)));
        let Request::Sound(SoundOrder::Play { data, slot }) = voice[1] else {
            panic!("a play: {voice:?}");
        };
        assert_eq!(slot, Some(SoundSlot::StoryVoice));
        assert_eq!(
            (data.kind, data.looping),
            (PlayType::AllExclusive, Looping::Inherit)
        );
        assert_eq!(story(story::Sound::Voice(None)).len(), 1, "only a stop");
    }
}
