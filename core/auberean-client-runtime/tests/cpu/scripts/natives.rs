//! The natives the client binds over the shared ones.

use std::collections::BTreeSet;

use auberean_script::library::Library;
use auberean_script::Binding;

/// The shared natives leave to a host's own model exactly the natives the client binds from its
/// model: each is bound there, and the client binds no other over a stand-in of the shared
/// natives (the panel notices aside, which its recorder stands in for).
///
/// Behaviour: script.natives.the-stand-ins-left-to-a-host-are-the-ones-the-client-binds
#[test]
fn the_stand_ins_left_to_a_host_are_the_natives_the_client_binds_from_its_model() {
    let lib = Library::default();
    let shared = auberean_glue::natives(&lib);
    let model = auberean_client_runtime::host::model_natives(&lib);
    let left: BTreeSet<u32> = auberean_glue::stubs::host_bound().map(|n| n.0).collect();
    let bound: BTreeSet<u32> = shared
        .iter()
        .filter(|(id, b)| {
            matches!(b, Binding::NotPorted(_)) && matches!(model.get(*id), Some(Binding::Ported(_)))
        })
        .map(|(id, _)| id)
        .collect();
    assert!(!left.is_empty());
    assert_eq!(left, bound);
}
