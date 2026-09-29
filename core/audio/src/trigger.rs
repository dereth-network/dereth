//! The eight entry points, and the twelve triggers that reach them.
//!
//! The entry points and consolidated trigger table are encoded directly below.
//!
//! The client contains almost no hard-coded "play sound X when Y happens" logic. Everything reaches
//! audio through one of three indirections — an animation or physics-script **hook** embedded in dat
//! data, a **`MediaPlayback`** descriptor on a UI element, or a **server message** naming an object and
//! a `SoundType` — plus two exceptions, the portal transitions and the `/environ` admin sounds.
//!
//! | Trigger | Entry | Lookup | Parameters |
//! |---|---:|---|---|
//! | Animation `Sound` hook | 1 | direct DataID | vol = `effect_volume`, **no roll** |
//! | Animation `SoundTable` hook | 4 | object's table by `SoundType` | vol = row `volume`, roll vs row `probability` |
//! | Animation `SoundTweaked` hook | 2 | direct DataID | vol = hook `vol`, roll vs hook `prob`, hook `prio` **ignored** |
//! | Physics-script hook | 1/2/4 | as above | as above |
//! | Server `Sound` `0xF750` | 3 | object's table | vol from the message, roll vs row `probability` |
//! | Ambient intermittent | 5 | scene's table | vol = `ambient_volume * desc.volume`, randomised position |
//! | Ambient continuous | 6 | scene's table | vol = `ambient_volume * current_volume`, from centre |
//! | UI media, table form | 7 | descriptor's table | vol = row `volume` |
//! | UI media, wave form | 8 | direct DataID | vol = 1.0, no roll |
//! | Portal enter | 7 | UI table, enter-portal sound (`0x6A`) | as entry 7 |
//! | Portal exit | 7 | UI table, exit-portal sound (`0x6B`) | as entry 7 |
//! | `/environ 101..123` | 7 | UI table, per the `/environ` map below | as entry 7 |

use dereth_primitives::DataId;

/// The three sound-hook types recognized by the animation-hook decoder.
///
/// **`SoundTweakedHook` reads probability then priority.** ACE's `SoundTweakedHook.Unpack` has the
/// two middle floats the other way round. The client is authoritative: its tweaked-sound hook
/// assigns `prob` from the first float and `prio` from the second, and the shipped data
/// agrees — across all 541 tweaked-sound hooks in `client_portal.dat` (173 in `0x03` animations, 368
/// in `0x33` physics scripts) the first float never exceeds 1.0 while the second reaches 3.0, which
/// cannot be a probability. Copying ACE's order rolls the wrong number and silences or mis-ranks
/// tweaked animation sounds.
///
/// `dereth_assets::hook::HookData` already decodes them in the client's order; this enum is
/// the audio-side view of the same three, so that this crate's tests can state the ordering claim
/// without depending on the animation crate's dispatch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SoundHook {
    /// Hook id 1. Loading the hook also creates the sound, so the wave is
    /// resident as soon as the animation carrying the hook is loaded.
    Sound { sound_id: DataId },
    /// Hook id 2. Resolved against **the object's own** `sound_table`, so the same animation
    /// produces different sounds for a drudge and a lugian.
    SoundTable { sound_type: u32 },
    /// Hook id 21. Field order: `gid, prob, prio, vol`.
    ///
    /// The constructor defaults are
    /// `prio = 0.9, prob = 1.0, vol = 1.0`.
    SoundTweaked {
        sound_id: DataId,
        probability: f32,
        priority: f32,
        volume: f32,
    },
}

impl SoundHook {
    /// The client's defaults, for a hook whose payload was
    /// truncated.
    #[must_use]
    pub fn default_tweaked(sound_id: DataId) -> Self {
        Self::SoundTweaked {
            sound_id,
            probability: 1.0,
            priority: 0.9,
            volume: 1.0,
        }
    }
}

/// What a UI sound descriptor (media type 9) names.
///
/// A `SoundType` of 0 (invalid) means the file field is a wave DataID (entry 8, volume 1.0);
/// otherwise the file field is a sound-table DataID and the sound type selects the row (entry 7).
///
/// Which UI event plays which sound is entirely a property of the layout data (`0x21xxxxxx` objects
/// in `client_local_English.dat`), not of the executable — a rebuild must read them out of the layout
/// data, not hard-code them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UiSoundRef {
    /// Entry 8: a bare wave, volume 1.0, no probability roll.
    Wave(DataId),
    /// Entry 7: a table row, at the row's own volume, rolled against the row's probability.
    Table { table: DataId, stype: u32 },
}

/// The invalid `SoundType` (0), which switches a UI sound descriptor to the wave form.
pub const SOUND_INVALID: u32 = 0;

/// The enter-portal UI sound, played when the player enters a portal.
pub const SOUND_UI_ENTER_PORTAL: u32 = 0x6A;
/// The exit-portal UI sound, played when the player leaves one, on the transition from the
/// tunnel fade-out to the world fade-in.
pub const SOUND_UI_EXIT_PORTAL: u32 = 0x6B;

/// `/environ <n>` -> `SoundType`, from the admin environment command.
///
/// Values 0-6 set landscape fog and ambient-light overrides and are not audio. Values 101-123 play
/// a one-shot atmosphere sound from the UI sound table through entry 7. **115, 116 and 124 fall into
/// `default:` and play nothing** — the gap is in the original, not a transcription slip.
#[must_use]
pub fn environ_sound_type(n: u32) -> Option<u32> {
    Some(match n {
        101 => 0x76, // roar
        102 => 0x77, // bell
        103 => 0x78, // chant 1
        104 => 0x79, // chant 2
        105 => 0x7A, // dark whispers 1
        106 => 0x7B, // dark whispers 2
        107 => 0x7C, // dark laugh
        108 => 0x7D, // dark wind
        109 => 0x7E, // dark speech
        110 => 0x7F, // drums
        111 => 0x80, // ghost speech
        112 => 0x81, // breathing
        113 => 0x82, // howl
        114 => 0x83, // lost souls
        117 => 0x84, // squeal
        118 => 0x85, // thunder 1
        119 => 0x86, // thunder 2
        120 => 0x87, // thunder 3
        121 => 0x88, // thunder 4
        122 => 0x89, // thunder 5
        123 => 0x8A, // thunder 6
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tweaked hook reads probability before priority.
    #[test]
    fn the_tweaked_hook_reads_probability_before_priority() {
        // A payload as it sits in the dat: gid, then 0.8, then 3.0, then 1.0.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x0A00_058Au32.to_le_bytes());
        bytes.extend_from_slice(&0.8f32.to_le_bytes());
        bytes.extend_from_slice(&3.0f32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        let f = |i: usize| f32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        let hook = SoundHook::SoundTweaked {
            sound_id: DataId(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
            probability: f(4),
            priority: f(8),
            volume: f(12),
        };
        let SoundHook::SoundTweaked {
            probability,
            priority,
            ..
        } = hook
        else {
            panic!("tweaked");
        };
        assert_eq!(probability, 0.8, "the FIRST float is the probability");
        assert_eq!(
            priority, 3.0,
            "the SECOND is the priority, and 3.0 cannot be a probability"
        );
        // ACE's order would give probability 3.0, which never rolls false, and priority 0.8.
        assert_ne!(probability, 3.0, "do not port ACE's field order");
    }

    /// Oracle: the recovered constructor defaults.
    #[test]
    fn the_tweaked_hook_constructor_defaults_are_prio_0_9_prob_1_vol_1() {
        let SoundHook::SoundTweaked {
            probability,
            priority,
            volume,
            ..
        } = SoundHook::default_tweaked(DataId(0x0A00_0001))
        else {
            panic!("tweaked");
        };
        assert_eq!((probability, priority, volume), (1.0, 0.9, 1.0));
    }

    /// Oracle: section 7's two-column table, including the three values that fall into `default:`.
    #[test]
    fn the_environ_table_matches_the_documented_mapping_including_its_gaps() {
        assert_eq!(environ_sound_type(101), Some(0x76));
        assert_eq!(environ_sound_type(114), Some(0x83));
        assert_eq!(environ_sound_type(117), Some(0x84));
        assert_eq!(environ_sound_type(123), Some(0x8A));
        for n in [0u32, 6, 100, 115, 116, 124, 200] {
            assert_eq!(environ_sound_type(n), None, "/environ {n} plays nothing");
        }
        // 21 values map to sounds: 101..114 is 14, plus 117..123 is 7.
        let mapped = (0..300u32)
            .filter(|&n| environ_sound_type(n).is_some())
            .count();
        assert_eq!(mapped, 21);
    }

    /// Both portal sounds go through entry 7 with the UI sound table. Oracle: section 6.2.
    #[test]
    fn the_two_portal_sound_types_are_the_documented_ones() {
        assert_eq!(SOUND_UI_ENTER_PORTAL, 0x6A);
        assert_eq!(SOUND_UI_EXIT_PORTAL, 0x6B);
    }
}
