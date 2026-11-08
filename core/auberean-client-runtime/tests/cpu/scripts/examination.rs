//! An examination profile takes the tabs its whole request asks for.

use auberean_glue::examination::{kind, Profile};
use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::value::tag;

use crate::common::Bench;

const REQUEST: u32 = 0x9A;
const PROFILE: u32 = 0x99;
/// A request about a recipe: its packed form names the recipe and no tabs.
const RECIPE: u32 = 0x4000_0007;

/// A profile set to a request about a recipe asking for `tabs`, given a line on the first tab and a
/// line on the details tab, as a recipe's scripts give.
fn profile_asking(tabs: Option<u32>) -> Profile {
    let mut b = Bench::new();
    let bytes = [0u32, 0, 0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();
    let q = b.vm.heap.add_native(
        native_type::EXAMINATION_REQUEST,
        NativeObject::Packed {
            ty: native_type::EXAMINATION_REQUEST,
            bytes,
        },
    );
    let ty = tag::INT;
    b.call(REQUEST, 0x06, q, &[(RECIPE, ty)], 0);
    b.call(REQUEST, 0x0B, q, &[(0x6F00_0F71, ty)], 0);
    if let Some(tabs) = tabs {
        b.call(REQUEST, 0x11, q, &[(tabs, ty)], 0);
    }
    let p =
        b.vm.heap
            .add_native(native_type::EXAMINATION_PROFILE, Profile::new().native());
    b.call(PROFILE, 0x03, p, &[(q, tag::REFERENCE)], 0);
    b.call(PROFILE, 0x08, p, &[(1, ty), (0xAB, ty), (0x02, ty)], 0);
    b.call(PROFILE, 0x08, p, &[(2, ty), (0xAB, ty), (0x08, ty)], 0);
    Profile::of(b.vm.heap.native(p).expect("the profile")).expect("a profile")
}

/// A profile set to a request about anything but an object keeps the lines of the tabs that
/// request asks for, though its packed form does not say them; the window reads the same tabs.
///
/// Behaviour: script.natives.examination-profile-takes-the-whole-requests-tabs
#[test]
fn a_profile_keeps_the_lines_of_the_tabs_its_whole_request_asks_for() {
    let all = profile_asking(Some(0x3FF));
    assert_eq!(all.asked(), 0x3FF);
    assert!(all.nodes.iter().all(|n| n.kind == kind::INT));
    let lines: Vec<u64> = all.nodes.iter().map(|n| n.value).collect();
    assert_eq!(lines, vec![1, 2], "the details tab's line stays");
    let first = profile_asking(Some(0x203));
    let lines: Vec<u64> = first.nodes.iter().map(|n| n.value).collect();
    assert_eq!(lines, vec![1], "only the first tab's");
    assert_eq!(first.tabs, 0x02 | 0x08, "both tabs are noted");
}
