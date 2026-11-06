//! The data files' physics script and sound numbers.
//!
//! A physics script table and a sound table key their rows by a number, and a message or a spell
//! names a row by that number. `portal.dat` files number them differently from the `client_*.dat`
//! files: they have no row for script 30 (a later file's second black skill drop), so every later
//! script from 31 up is one lower in them, and their sounds from raising a stat to the last
//! trigger sounds sit 0x15 lower. A client looks up what it receives as sent, so whoever names a
//! later number on such files says it in their numbering first: a server working in later numbers
//! when it sends, the client for the scripts it asks for itself.

use dereth_primitives::ContainerEra;

/// The script number `portal.dat` files have no row for.
const SCRIPT_MISSING: u32 = 30;
/// The sounds that sit lower in `portal.dat` files.
const FIRST_MOVED_SOUND: u32 = 0x8B;
const LAST_MOVED_SOUND: u32 = 0x9A;
/// How far lower they sit.
const SOUND_SHIFT: u32 = 0x15;

/// The number files of `era` give the later script `script`; `None` when they have no such row.
#[must_use]
pub const fn script_in_files(era: ContainerEra, script: u32) -> Option<u32> {
    match era {
        ContainerEra::Modern => Some(script),
        ContainerEra::Classic => {
            if script < SCRIPT_MISSING {
                Some(script)
            } else if script == SCRIPT_MISSING {
                None
            } else {
                Some(script - 1)
            }
        }
    }
}

/// The later number of the script that files of `era` number `script`.
#[must_use]
pub const fn script_from_files(era: ContainerEra, script: u32) -> u32 {
    match era {
        ContainerEra::Classic if script >= SCRIPT_MISSING => script + 1,
        _ => script,
    }
}

/// The number files of `era` give the later sound `sound`.
#[must_use]
pub const fn sound_in_files(era: ContainerEra, sound: u32) -> u32 {
    match era {
        ContainerEra::Classic if FIRST_MOVED_SOUND <= sound && sound <= LAST_MOVED_SOUND => {
            sound - SOUND_SHIFT
        }
        _ => sound,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the arithmetic; the data files' own rows are checked where they are read)
    use super::*;

    #[test]
    fn later_files_keep_every_number() {
        for n in [0, 30, 31, 0x73, 0x8F, 0xCC] {
            assert_eq!(script_in_files(ContainerEra::Modern, n), Some(n));
            assert_eq!(script_from_files(ContainerEra::Modern, n), n);
            assert_eq!(sound_in_files(ContainerEra::Modern, n), n);
        }
    }

    #[test]
    fn older_files_have_no_script_30_and_number_the_later_ones_one_lower() {
        let classic = ContainerEra::Classic;
        assert_eq!(script_in_files(classic, 29), Some(29));
        assert_eq!(script_in_files(classic, 30), None);
        assert_eq!(script_in_files(classic, 31), Some(30));
        assert_eq!(script_in_files(classic, 0x73), Some(0x72));
        for later in [0, 29, 31, 0x73, 0x8D] {
            let files = script_in_files(classic, later).expect("a row");
            assert_eq!(script_from_files(classic, files), later);
        }
    }

    #[test]
    fn older_files_number_the_stat_to_trigger_sounds_0x15_lower() {
        let classic = ContainerEra::Classic;
        assert_eq!(sound_in_files(classic, 0x8A), 0x8A);
        assert_eq!(sound_in_files(classic, 0x8B), 0x76);
        assert_eq!(sound_in_files(classic, 0x8F), 0x7A);
        assert_eq!(sound_in_files(classic, 0x97), 0x82);
        assert_eq!(sound_in_files(classic, 0x9A), 0x85);
        assert_eq!(sound_in_files(classic, 0x9B), 0x9B);
    }
}
