//! The identifiers the client asks for, as text.

use auberean_friends_dll::ids::{self, Face};

/// Behaviour: friends.library.identifiers
#[test]
fn the_class_and_the_ten_interfaces_are_the_identifiers_the_client_asks_for() {
    // As text, in lower case.
    let text = |g: ids::Guid| g.to_string().to_ascii_lowercase();
    assert_eq!(text(ids::CLASS), "{76633332-52a2-4460-b089-3541a8443c9a}");
    assert_eq!(text(ids::UNKNOWN), "{00000000-0000-0000-c000-000000000046}");
    assert_eq!(
        text(ids::CLASS_FACTORY),
        "{00000001-0000-0000-c000-000000000046}"
    );
    let asked: Vec<String> = Face::ALL.iter().map(|f| text(f.id())).collect();
    assert_eq!(
        asked,
        [
            "{eeeeee21-85ea-40f7-8157-cd07a138b49f}",
            "{eeeeee23-85ea-40f7-8157-cd07a138b49f}",
            "{eeeeee22-85ea-40f7-8157-cd07a138b49f}",
            "{eeeeee25-85ea-40f7-8157-cd07a138b49f}",
            "{eeeeee24-85ea-40f7-8157-cd07a138b49f}",
            "{eeeeee26-85ea-40f7-8157-cd07a138b49f}",
            "{eeeeee27-85ea-40f7-8157-cd07a138b49f}",
            "{eb60ba8a-54a6-4a6d-9432-8337dfff7612}",
            "{c4ef31f6-6d0a-4e84-a67b-cf97a22c57a2}",
            "{eeeeee29-85ea-40f7-8157-cd07a138b49f}",
        ],
        "in the order the client queries them"
    );
    for (i, face) in Face::ALL.into_iter().enumerate() {
        assert_eq!(face.index(), i);
        assert_eq!(Face::of(&face.id()), Some(face));
    }
    assert_eq!(Face::of(&ids::UNKNOWN), Some(Face::Connection));
    assert_eq!(Face::of(&ids::CLASS), None);
    let slots: Vec<usize> = Face::ALL.iter().map(|f| f.slots()).collect();
    assert_eq!(slots, [9, 10, 4, 23, 13, 14, 7, 8, 11, 5]);
}
