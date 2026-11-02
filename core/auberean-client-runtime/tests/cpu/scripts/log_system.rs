//! The scripts' log system: the masks of the kinds of log line kept, read and changed.

use auberean_script::value::{tag, NULL};

use crate::common::Bench;

const LOG_SYSTEM: u32 = 0x75;

/// The file's mask starts at 0x1A4 and the console's at 0; a kind is logged when the mask holds
/// all of it (kind 4 always to the file); setting replaces a mask, adding sets bits, removing
/// clears them.
///
/// Behaviour: script.model.the-log-masks-start-and-change-as-the-scripts-say
#[test]
fn the_log_masks_start_at_the_clients_and_change_as_the_scripts_say() {
    let mut b = Bench::new();
    let int = tag::INT;
    let get = |b: &mut Bench, n: u32| b.answer(LOG_SYSTEM, n, NULL, &[]);
    assert_eq!(get(&mut b, 0x09), 0x1A4, "the file's");
    assert_eq!(get(&mut b, 0x0A), 0, "the console's");
    let is = |b: &mut Bench, n: u32, kind: u32| {
        let (_, slots) = b.call(LOG_SYSTEM, n, NULL, &[(kind, int)], 1);
        slots[0].0
    };
    assert_eq!(is(&mut b, 0x01, 0x24), 1, "all its bits are in the file's");
    assert_eq!(is(&mut b, 0x01, 0x21), 0, "not all");
    assert_eq!(is(&mut b, 0x01, 4), 1, "kind 4 always");
    assert_eq!(is(&mut b, 0x02, 4), 0);
    b.call(LOG_SYSTEM, 0x04, NULL, &[(0x0C, int)], 0);
    assert_eq!(is(&mut b, 0x02, 4), 1);
    assert_eq!(get(&mut b, 0x0A), 0x0C, "added");
    b.call(LOG_SYSTEM, 0x08, NULL, &[(0x04, int)], 0);
    assert_eq!(get(&mut b, 0x0A), 0x08, "removed");
    b.call(LOG_SYSTEM, 0x06, NULL, &[(0x30, int)], 0);
    assert_eq!(get(&mut b, 0x0A), 0x30, "set");
    b.call(LOG_SYSTEM, 0x03, NULL, &[(0x01, int)], 0);
    assert_eq!(get(&mut b, 0x09), 0x1A5);
    b.call(LOG_SYSTEM, 0x07, NULL, &[(0x1A0, int)], 0);
    assert_eq!(get(&mut b, 0x09), 0x05);
    b.call(LOG_SYSTEM, 0x05, NULL, &[(0x2, int)], 0);
    assert_eq!(get(&mut b, 0x09), 0x02);
    assert_eq!(is(&mut b, 0x01, 4), 1, "kind 4 still");
}
