//! Audio -- what the client plays, and when.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "audio.ambient.the-regions-ambience-reaches-the-mixer",
        says: "Standing in Holtburg, the region's ambient sounds start playing and reach the \
               speakers as audible sound, not silence.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-WORLD-AUDIO-AMBIENT"),
        station: "dereth-client::gpu::audio::world_audio::holtburgs_ambience_reaches_the_mixer_as_samples",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "audio.environs.an-admin-environs-sound-reaches-the-mixer",
        says: "A sound effect the shard's environment message asks for plays as one centred sound \
               that is actually heard, while the same message is silent before the player has a \
               body, for the values the retail client has no sound for, and when it arrives cut \
               short.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-ENVIRONS"),
        station: "dereth-client::dat::audio::admin_environs_sound::an_encoded_admin_environs_sound_reaches_the_mixer_and_keeps_the_native_guards",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.listener.follows-the-camera",
        says: "Sounds are heard from where the camera is, updated every frame as the player runs, \
               not from where the player's body stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-LISTENER"),
        station: "dereth-client::gpu::audio::ui_and_script_sounds::the_listener_follows_the_camera_every_frame",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "audio.mixer.attenuation-and-pan-follow-the-recovered-tables",
        says: "A sound plays at full volume within 5 metres, then falls by 12 decibels each time \
               its distance doubles, in whole-decibel steps, and past about 94 metres it is not \
               played at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AUDIO-CONFORMANCE-MIXER"),
        station: "dereth-audio::dat::audio::audio_conformance::the_attenuation_sweep_is_exact",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.movie.the-intro-movie-soundtrack-plays-regardless-of-sound-preferences",
        says: "The opening movie's soundtrack reaches the sound device loudly even with every \
               volume at zero and every sound category switched off, because the game's sound \
               options do not apply to the movie.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O906-MOVIE"),
        station: "dereth-client::dat::audio::device_output_level::the_movie_soundtrack_reaches_the_device_and_ignores_the_sound_preferences",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.output.the-startup-sound-reaches-the-device-buffer-audibly",
        says: "The button sound the client plays as it starts reaches the sound device at an \
               audibly loud level after it is converted to the device's rate.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O906-OUTPUT"),
        station: "dereth-client::dat::audio::device_output_level::the_startup_ui_sound_reaches_the_device_boundary_audibly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.sound-event.every-recorded-sound-names-an-object-the-session-created",
        says: "Every sound the shard plays at an object in the recorded sessions names an object \
               that the same session creates, so a sound never has to wait on an object the client \
               has never heard of, and each one decodes with the client's reading of the message.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O239-SOUND-EVENT-EVERY"),
        station: "dereth-client-net::cpu::audio::sound_event_corpus::every_sound_event_names_an_object_its_own_capture_created",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.sound-event.plays-the-objects-table-at-the-messages-volume",
        says: "A sound event from the server plays from the object's own sound table at the volume \
               the message carries, not at the volume the table's entry would give it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O239-SOUND-EVENT"),
        station: "dereth-client::gpu::audio::server_sound_event::the_messages_volume_overrides_the_rows",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "audio.sound.a-triggered-sound-is-attenuated-and-panned",
        says: "A sound triggered in the world gets quieter with distance and is panned toward its \
               side: louder on the left for a sound to the west and on the right for one to the \
               east.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-WORLD-AUDIO-SOUND"),
        station: "dereth-client::gpu::audio::world_audio::a_triggered_sound_is_attenuated_by_distance_and_panned_by_bearing",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "audio.ui.a-button-press-plays-the-layouts-click-sound",
        says: "Pressing a button with the pointer plays the click sound that button's own layout \
               names, and the sound is actually heard, not merely started.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-UI"),
        station: "dereth-client::gpu::audio::ui_and_script_sounds::a_real_button_press_plays_the_layouts_own_click_sound",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "audio.voices.the-seventeenth-voice-is-dropped",
        says: "At most sixteen sounds play at once: they take the sixteen voices in turn, and a \
               seventeenth started while all sixteen are busy is dropped rather than cutting off \
               one already playing, whatever its priority.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AUDIO-CONFORMANCE-VOICES"),
        station: "dereth-audio::dat::audio::audio_conformance::the_voice_pool_allocates_round_robin_and_drops_the_seventeenth",
        tier: Tier::Dat,
    },
];
