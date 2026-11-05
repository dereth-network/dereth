//! Sound: the sound manager every sound passes through, the rules of the sound and music
//! settings, the volume and positional laws, and what plays the sounds.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "audio.ambient.a-direction-is-a-compass-eighth",
        says: "A place's direction from the player is the eighth of its compass heading, clockwise from north; a place too near to point counts as north, and one at the player weighs 1 spread over all eight.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-DIRECTION"),
        station: "auberean-audio::cpu::ambient::a_direction_is_its_compass_eighth_clockwise_from_north",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.ambient.a-place-weighs-by-its-level-distance",
        says: "An ambient place weighs 1 within 12 meters of the player and 144 over its distance squared beyond, its height not counted, half when muffled; past 100 meters a sound does not count it, and it adds to the whole only by the inverse square.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-WEIGHTS"),
        station: "auberean-audio::cpu::ambient::a_place_weighs_fully_within_twelve_meters_and_by_the_inverse_square_beyond",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.ambient.a-session-hears-its-place",
        says: "In a session in the hall, the ambient kind most of its cells name dominates and its place                music plays from an ambient sound's object, its piece started on the performance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-SESSION"),
        station: "auberean-shell::dat::session::in_the_hall_the_interiors_place_music_plays",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.ambient.cells-and-the-landscape",
        says: "Every interior cell in view names the ambient kind its own properties say, kind 2 when they say none; the landscape's points are weighed only when the landscape is heard, muffled from indoors, and a place's kind is described the first time it is met.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-PLACES"),
        station: "auberean-audio::cpu::ambient::interior_cells_name_their_kind_and_the_landscape_is_heard_only_when_it_can_be",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.ambient.dominance-moves-with-the-player",
        says: "When another description comes to dominate, the old one (still dominant while the shares are worked out) asks for its music again, the new one's music plays, and the old one's stops unless it plays on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-DOMINANCE-MOVES"),
        station: "auberean-audio::cpu::ambient::when_dominance_moves_the_new_music_plays_and_the_old_stops_unless_it_plays_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.ambient.heard-from-the-camera",
        says: "An ambient sound's object stands where the camera was last drawn from, plus its offset.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-CAMERA"),
        station: "auberean-audio::cpu::ambient::the_place_music_stands_where_the_camera_was_drawn_from",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.ambient.intermittent-sounds-play-now-and-then",
        says: "An intermittent ambient sound plays no sooner than its time, then waits a random time within its range; it plays by a roll within its probability times its share, at most one of its description a turn, from a place in a direction its densities pick, at its pitch.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-INTERMITTENT"),
        station: "auberean-audio::cpu::ambient::an_intermittent_sound_waits_its_time_and_plays_by_chance_one_a_turn",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.ambient.leaving-the-world-stops-it",
        says: "Leaving the world lets every ambient description go, stops the music and the sounds heard everywhere, and the next update weighs the places afresh.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-CLEAR"),
        station: "auberean-audio::cpu::ambient::clearing_lets_every_description_go_and_stops_the_music_and_flat_sounds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.ambient.no-table-no-ambient",
        says: "With no ambient sound table the ambient sound turns itself off at the first place it weighs; setting a region with a table turns it on again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-NO-TABLE"),
        station: "auberean-audio::cpu::ambient::with_no_ambient_table_the_ambient_sound_turns_itself_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.ambient.place-music-plays-in-the-world",
        says: "Standing outdoors in the end-of-retail world, the description of the place's                ambient kinds with the greatest share dominates, and its place music plays through                the sound manager on the primary track, its piece started on the performance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-IN-THE-WORLD"),
        station: "auberean-audio::dat::ambient::standing_outdoors_the_dominant_places_music_plays_on_the_primary_track",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.ambient.quiet-descriptions-go",
        says: "A description whose share falls below a hundredth falls quiet (its continuous sounds stop unless they play on) and goes at the next half-second check.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-QUIET"),
        station: "auberean-audio::cpu::ambient::a_description_heard_below_a_hundredth_falls_quiet_and_goes_at_the_next_check",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.ambient.recomputed-after-four-meters",
        says: "The ambient places are weighed again only once the player has moved 4 meters since the last time, and never while the cells about it are waited for.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-RECOMPUTE"),
        station: "auberean-audio::cpu::ambient::the_places_are_weighed_again_only_after_four_meters_and_never_while_cells_are_waited_for",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.ambient.the-dominant-description-plays-its-music",
        says: "The description with the greatest share dominates the place; its place music, which waits for that, plays on the music channel at the share as its volume, made by its own object.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-DOMINANT"),
        station: "auberean-audio::cpu::ambient::the_description_with_the_greatest_share_dominates_and_plays_its_place_music",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.effects.an-effect-plays-its-sound-from-its-object",
        says: "An effect played on an object plays its sound (one of its list at random), made by \
               the object, heard from where it is, from the frame it is played; a looping one is \
               kept to stop with the effect.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-EFFECTS"),
        station: "auberean-shell::dat::session::effects_played_on_objects_play_their_sounds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.manager.a-silent-makers-sound-takes-no-budget",
        says: "A sound that plays one at a time and counts against the budget takes no place in \
               it while its maker is silent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-BUDGET-MUTED"),
        station: "auberean-audio::cpu::manager::a_silent_makers_one_at_a_time_sound_takes_no_place_in_the_budget",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.a-silent-makers-sounds-stop-and-play-again",
        says: "A sound whose maker has gone silent stops and is kept; it plays again when its \
               maker can be heard. Sounds of other makers are not touched.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-MAKER-SILENT"),
        station: "auberean-audio::cpu::manager::a_silent_makers_sounds_stop_and_play_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.a-sound-starts-on-the-next-turn-at-its-level",
        says: "A sound asked for starts on the sound manager's next turn, not at once, and plays \
               its wave as recorded at full volume on a full slider; at half volume it plays 12.5 \
               decibels down; once its wave has ended it is finished.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-PLAY-PATH"),
        station: "auberean-audio::cpu::manager::a_sound_starts_on_the_next_turn_and_plays_its_wave_at_its_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.a-sounds-frequency-changes-while-it-plays",
        says: "A sound on a path of sounds takes a new frequency while it plays (its rate the buffer's times it); a change below 0.0002 or a negative one is ignored, as is one before it has a voice.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-FREQUENCY"),
        station: "auberean-audio::cpu::manager::a_playing_sounds_frequency_changes_its_rate",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.a-stopped-primary-piece-waits-no-longer",
        says: "A piece waiting for the primary track that is stopped waits no longer, and the one \
               playing starts again when it ends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-STOPPED-QUEUE"),
        station: "auberean-audio::cpu::music::a_waiting_primary_piece_that_is_stopped_leaves_the_one_playing_to_repeat",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.a-track-plays-through-the-manager",
        says: "An encoded track's music setting plays through the sound manager from the next \
               turn, and is heard.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-TRACK-PLAYS"),
        station: "auberean-audio::dat::records::a_track_plays_through_the_manager_on_the_next_turn",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.manager.alone",
        says: "A sound that plays alone silences every other sound and the music while it plays; \
               when it is done the others play again and the music slider is back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-ALONE"),
        station: "auberean-audio::cpu::manager::a_sound_that_plays_alone_silences_the_others_and_the_music",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.fade",
        says: "A sound stopped with a fade falls in decibels to silence over the fade, and is \
               finished once the fade has run.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-FADE"),
        station: "auberean-audio::cpu::manager::a_stop_with_a_fade_falls_to_silence_and_then_ends",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.instrument-music-plays",
        says: "A music setting of music made of instruments plays its piece through the mixer \
               from the next turn, placed or heard everywhere as its setting says, and is done \
               when the piece ends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-MUSIC-PLAYS"),
        station: "auberean-audio::cpu::music::a_music_setting_of_instruments_plays_through_the_mixer_and_ends_with_its_piece",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.music-off",
        says: "With music turned off, every piece of music playing stops and music asked for on \
               the music or player music channel is refused; music on the main channel still \
               plays.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-MUSIC-OFF"),
        station: "auberean-audio::cpu::manager::music_off_refuses_music_on_the_music_channels_and_stops_what_plays",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.no-device-plays-nothing",
        says: "With no sound device every sound asked for is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-NO-DEVICE"),
        station: "auberean-audio::cpu::manager::with_no_device_every_request_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.one-at-a-time",
        says: "A setting that plays one at a time is not played again while it plays, and plays \
               again once it has gone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-ONE-AT-A-TIME"),
        station: "auberean-audio::cpu::manager::a_self_exclusive_setting_plays_once_at_a_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.placed-sounds-follow-their-maker",
        says: "A sound placed in the world is heard from where the object making it is, by \
               distance, side and speed, and follows it a tenth of a second at a time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-3D"),
        station: "auberean-audio::cpu::manager::a_placed_sound_follows_its_maker_a_tenth_of_a_second_at_a_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.primary-track-queue",
        says: "Pieces on the primary track play one at a time: the one playing starts again when \
               it ends while none waits; one asked for meanwhile waits for it to end and then \
               replaces it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-MUSIC-THREAD"),
        station: "auberean-audio::cpu::music::a_primary_piece_waits_for_the_one_playing_which_repeats_until_another_waits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.sliders-reach-sounds-playing",
        says: "A volume slider moved, or a sound's own volume changed, is heard at once in the \
               sounds already playing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-SLIDERS"),
        station: "auberean-audio::cpu::manager::a_slider_change_reaches_the_sounds_playing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.the-budget-refuses-the-thirty-third",
        says: "At most 32 sounds counted against the budget play at once: past that a sound that \
               does not loop is refused or stopped, while looping sounds and sounds not counted \
               are not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-BUDGET"),
        station: "auberean-audio::cpu::manager::a_non_looping_counted_sound_past_the_budget_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.manager.too-far",
        says: "A sound 250 meters away or more does not play: one that does not loop stops for \
               good, a looping one pauses and plays again when its maker comes nearer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-TOO-FAR"),
        station: "auberean-audio::cpu::manager::a_sound_250_meters_away_stops_and_a_looping_one_pauses",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.chord.music-values",
        says: "A style note's music value becomes its key as the music's runtime converts it: \
               the octave from one below the root, the chord position as the chord's note (past \
               its notes, two scale steps a position), the scale position as scale steps, the \
               accidental in semitones; the fixed mode's value is the key; a key out of range \
               comes in by octaves.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-MUSIC-VALUES"),
        station: "auberean-audio::cpu::music::a_style_notes_music_value_becomes_its_key_over_the_chord_and_scale",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.every-music-file-reads",
        says: "Every music file of the end-of-retail data reads by its form: 575 pieces, 379 \
               styles (1,164 patterns, 1,024 parts), 60 instrument collections (61 instruments, \
               334 regions, 319 waves, every region's wave in its pool), 13 scripts and 9 audio \
               path configurations.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-MUSIC-FILES"),
        station: "auberean-audio::dat::music::every_music_file_reads_by_its_form",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.music.every-piece-finds-its-needs",
        says: "Every piece finds every file it names (the pieces it starts, its styles, its \
               bands' collections, its scripts and what they hold) by file name through the \
               data's file table, and every track it holds is one the player reads.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-MUSIC-NEEDS"),
        station: "auberean-audio::dat::music::every_piece_finds_everything_it_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.music.every-place-script-runs",
        says: "Every place script runs: its one routine reads the master groove level and, at \
               every level from 0 to 100, plays exactly one piece the script holds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-SCRIPTS"),
        station: "auberean-audio::dat::music::every_place_script_runs_and_plays_a_piece_it_holds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.music.object-pieces-play-notes",
        says: "The pieces of the placed music settings (the creature and object sounds built \
               from instruments) play notes on their bands' instruments and are heard.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-OBJECT-PIECES"),
        station: "auberean-audio::dat::music::the_pieces_of_object_and_creature_sounds_play_notes_on_instruments",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.music.paths-compress",
        says: "Each music path has its configuration's compressor: above the threshold only a \
               ratio of the excess passes, the gain is added, and the sound is held back by the \
               predelay.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-PATH-FX"),
        station: "auberean-audio::cpu::music::a_music_paths_compressor_lets_through_a_ratio_of_the_excess_and_adds_its_gain",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.perf.band-before-notes",
        says: "A band a piece starts at its own start sets the channels' instruments before the \
               piece's first notes, so a note at time 0 sounds from the first frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-BANDS"),
        station: "auberean-audio::cpu::music::a_band_triggered_at_the_start_sets_the_instrument_before_the_first_note",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.perf.notes-on-time",
        says: "A sequence's note sounds at its music time at the tempo (768 ticks a quarter \
               note) and lasts its duration.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-MUSIC-TIME"),
        station: "auberean-audio::cpu::music::a_sequence_note_sounds_at_its_music_time_at_the_tempo",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.perf.piece-ends",
        says: "A piece that does not repeat is done at its length.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-PIECE-LENGTH"),
        station: "auberean-audio::cpu::music::a_piece_is_done_at_its_length",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.perf.script-chooses-by-groove",
        says: "A place's piece calls its script, which plays the piece its groove test chooses \
               (below 9, above 16, or between) after the one leading.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-GROOVE"),
        station: "auberean-audio::cpu::music::a_place_script_plays_the_piece_its_groove_test_chooses",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.perf.secondary-on-beat",
        says: "A piece started on the beat waits for the performance's next beat.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-BOUNDARIES"),
        station: "auberean-audio::cpu::music::a_piece_started_on_the_beat_waits_for_the_next_beat",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.perf.style-pattern-by-groove",
        says: "A style plays, of its patterns, one whose groove range holds the groove level, \
               and each part's notes on the part's channel.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-STYLES"),
        station: "auberean-audio::cpu::music::a_style_plays_the_pattern_whose_groove_range_holds_the_groove_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.synth.decay-to-sustain",
        says: "The volume envelope's decay falls 96 decibels per decay time until the sustain \
               level (a fraction of that range) and holds there while the key is down.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-DLS-DECAY"),
        station: "auberean-audio::cpu::music::the_decay_falls_to_the_sustain_level_and_holds_while_the_key_is_down",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.synth.loops-while-held",
        says: "A wave with a loop sounds past its length, looping, while its note is held.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-DLS-LOOPS"),
        station: "auberean-audio::cpu::music::a_looped_wave_sounds_past_its_length_while_the_key_is_held",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.synth.note-plays-its-region",
        says: "A note plays the wave of the region of its channel's instrument whose key range \
               holds it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-DLS-REGIONS"),
        station: "auberean-audio::cpu::music::a_note_plays_the_wave_of_the_region_that_holds_its_key",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.synth.pan",
        says: "A stereo path's synthesizer pans a channel by constant power; a placed path's is \
               mono.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-DLS-PAN"),
        station: "auberean-audio::cpu::music::a_stereo_synthesizer_pans_by_constant_power",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.synth.pitch-follows-the-key",
        says: "A note plays its wave a semitone per key from the wave's root note: an octave up \
               reads it twice as fast.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-DLS-PITCH"),
        station: "auberean-audio::cpu::music::a_note_an_octave_above_the_root_reads_its_wave_twice_as_fast",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.synth.velocity-and-volume-curve",
        says: "Velocity, volume and expression each set the level by 40 log10(v / 127) decibels: \
               the square of their fraction.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-DLS-CURVES"),
        station: "auberean-audio::cpu::music::velocity_and_volume_each_scale_the_level_by_the_square_of_their_fraction",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.music.synth.volume-envelope",
        says: "The volume envelope rises linearly in amplitude over its attack and, once the \
               note is released, falls 96 decibels over its release time, when the voice ends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-DLS-ENVELOPE"),
        station: "auberean-audio::cpu::music::the_volume_envelope_rises_through_its_attack_and_falls_96_db_over_its_release",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.options.playing-sounds-held-to-sixteen-to-thirty-two",
        says: "The number of sounds preference (8 to 64, 32 by default) sets the budget to its whole part, at most the device's 32 and at least 16, as it changes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-OPTIONS-COUNT"),
        station: "auberean-audio::cpu::manager::the_number_of_sounds_is_its_whole_part_held_between_sixteen_and_thirty_two",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.options.sliders-at-once-the-rest-when-applied",
        says: "The four volume sliders take effect as they change and not when applied; music off and the sound features take effect when applied; with no device nothing is taken.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-OPTIONS"),
        station: "auberean-audio::cpu::manager::the_sound_sliders_take_effect_as_they_change_and_the_rest_when_applied",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.options.sound-off-ends-what-is-not-playing",
        says: "With the sound off, a paused sound is done as it turns off, and music, having no \
               length of its own, is done on its next turn or when asked for.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-SOUND-OFF-ENDS"),
        station: "auberean-audio::cpu::manager::sound_off_ends_a_paused_sound_and_music_with_no_length_of_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.options.sound-off-silences-everything",
        says: "Sound off, applied, stops every sound playing and holds its voice no longer; on \
               again, each plays from its start, with any asked for meanwhile that has a length.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-OPTIONS-SOUND-OFF"),
        station: "auberean-audio::cpu::manager::sound_off_silences_every_sound_until_it_is_on_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.options.the-sound-section",
        says: "The sound section holds the sound manager's eight preferences in its order: the effects, music, player music and ambient volumes (0 to 1, full by default), the sound features (hardware by default), the number of sounds (8 to 64, 32), sound off and music off.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-OPTIONS-SECTION"),
        station: "auberean-audio::cpu::manager::the_sound_section_holds_eight_preferences_in_the_managers_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.records.a-track-reads-as-far-as-its-bytes-go",
        says: "An encoded track is a wave file read chunk by chunk as far as its bytes go, \
               whatever size the file says it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-TRACK-FILE"),
        station: "auberean-audio::cpu::records::an_encoded_track_reads_its_wave_file_as_far_as_its_bytes_go",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.records.a-waves-length-is-its-frames-over-its-rate",
        says: "A wave lasts its whole frames over its rate.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-WAVE-LENGTH"),
        station: "auberean-audio::cpu::records::a_waves_length_is_its_frames_over_its_rate",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.records.every-sound-setting-plays-its-wave",
        says: "Every one of the 1,970 sound settings of the end-of-retail set plays its wave, \
               which lasts as long as the setting says.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-RECORDS"),
        station: "auberean-audio::dat::records::every_sound_setting_plays_its_wave",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.records.every-track-decodes",
        says: "Of the end-of-retail set's music settings, the 37 encoded tracks all decode (over \
               50 minutes of music) and the 479 pieces of music made of instruments name no \
               track.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-TRACKS"),
        station: "auberean-audio::dat::records::every_encoded_track_decodes_and_instrument_music_has_none",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.screens.the-screens-play-their-music-and-the-world-stops-it",
        says: "Each screen before the world plays its music on the main channel, kept across the \
               screens, the music before it stopped over two seconds; the play screen stops it \
               the same way as it is made.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-SCREENS"),
        station: "auberean-interface::dat::screens::the_screens_ask_for_their_music_and_the_play_screen_stops_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "audio.settings.music-bits",
        says: "A music setting's bits give whether it loops (never on the primary track), whether \
               it plays one at a time, its track, when it starts and stops against the music, \
               whether it is a control piece, and whether it is placed in the world; an encoded \
               track not placed plays as a sound heard everywhere.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-INFO-BITS"),
        station: "auberean-audio::cpu::laws::a_music_settings_bits_give_its_track_type_timings_and_path",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.settings.sound-bits",
        says: "A sound setting's bits give whether it loops, whether it plays one at a time, and \
               whether it is placed in the world; it is always counted against the budget, starts \
               and stops at once, and a request's own choices win over its bits.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-SOUND-BITS"),
        station: "auberean-audio::cpu::laws::a_sound_settings_bits_give_its_looping_type_and_path",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.space.distance-law",
        says: "A placed sound is at full level within its setting's minimum distance and beyond it \
               falls 6 decibels for each doubling of the distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-DISTANCE"),
        station: "auberean-audio::cpu::laws::a_placed_sound_falls_6_decibels_a_doubling_beyond_its_minimum_distance",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.space.doppler",
        says: "A placed sound coming toward the listener is heard higher and one going away lower, \
               against a speed of sound of 343 meters a second; one crossing is not shifted.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-DOPPLER"),
        station: "auberean-audio::cpu::laws::a_placed_sound_closing_on_the_listener_is_heard_higher",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.space.pan-law",
        says: "A placed sound is panned to the side it is on, wholly into one speaker at a right \
               angle to the view, the height ignored and behind heard as ahead.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-PAN"),
        station: "auberean-audio::cpu::laws::a_placed_sound_pans_wholly_to_one_side_at_a_right_angle_and_ignores_height",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.space.place-in-the-listeners-frame",
        says: "A placed sound is heard from its place relative to the listener (the camera), \
               turned into the listener's own frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-LISTENER-FRAME"),
        station: "auberean-audio::cpu::laws::a_place_is_turned_into_the_listeners_frame",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.volume.level-is-linear-in-decibels",
        says: "A sound plays at a level 25 decibels times one less its slider times its volume \
               down, never louder than as recorded; a volume or slider of 0 is silent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-VOLUME"),
        station: "auberean-audio::cpu::laws::the_level_falls_25_decibels_over_slider_times_volume_and_0_is_silent",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "audio.volume.music-and-placed-paths-scale-the-slider",
        says: "The music slider counts for 0.4 of itself (music on a full slider plays 15 decibels \
               down) and a sound placed in the world for four times its slider.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-CHANNEL-LEVELS"),
        station: "auberean-audio::cpu::laws::the_music_slider_counts_for_04_and_a_placed_path_for_four",
        tier: Tier::Cpu,
    },
];
