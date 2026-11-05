//! Behaviour: none (tooling: the Asheron's Call 2 dialect of the shared pack format)

use std::path::Path;

use auberean_pack::{Ac2, Era, Item};
use dereth_pack::conflict::Scope;
use dereth_pack::pack::{PackHeader, Remove};
use dereth_pack::{Loc, World};

fn world(text: &str) -> Result<World<Ac2>, String> {
    World::<Ac2>::parse(Path::new("world.toml"), text).map_err(|e| e.to_string())
}

#[test]
fn a_world_of_the_game_names_it_and_the_end_of_retail() {
    let w = world("key = \"w\"\ngame = \"ac2\"\nera = \"eor\"\npacks = [\"a\"]\n").unwrap();
    assert_eq!(w.era, Era::Eor);
    assert_eq!(w.era.name(), "eor");
    let e = world("key = \"w\"\nera = \"eor\"\n").unwrap_err();
    assert!(e.contains("\"ac1\"") && e.contains("Yalain"), "{e}");
    let e = world("key = \"w\"\ngame = \"ac2\"\n").unwrap_err();
    assert!(e.contains("names its era"), "{e}");
    let e = world("key = \"w\"\ngame = \"ac2\"\nera = \"0930\"\n").unwrap_err();
    assert!(e.contains("\"0930\""), "{e}");
    // A world of this game may lie on a base database.
    let w = world("key = \"w\"\ngame = \"ac2\"\nera = \"eor\"\n[base]\ndb = \"yalain-world\"\n")
        .unwrap();
    assert_eq!(w.base.unwrap().db, "yalain-world");
}

#[test]
fn every_item_form_reads_and_is_written_one_way() {
    for (text, canonical) in [
        ("object s-5c@0298", "object s-5c@0298"),
        ("camp c-4586@77b8", "camp c-4586@77B8"),
        ("place p-75b90031", "place p-75b90031"),
        ("loot 0x19B", "loot 0x19b"),
        ("loot 411", "loot 0x19b"),
        ("keeper 0x470033D0", "keeper 0x470033d0"),
        ("sale 0x6F0016F6 3", "sale 0x6f0016f6 3"),
        ("quest 0x40000088", "quest 0x40000088"),
        ("step 0x40000088 s1", "step 0x40000088 s1"),
        ("give mirra-beryl", "give mirra-beryl"),
        ("talk t-335b-1cf7eb76", "talk t-335b-1cf7eb76"),
        ("route patrol-1", "route patrol-1"),
        ("area 41aa", "area 41AA"),
    ] {
        let words: Vec<&str> = text.split_whitespace().collect();
        let item = Item::parse(&words).unwrap_or_else(|| panic!("{text}"));
        assert_eq!(item.to_string(), canonical, "{text}");
        assert_eq!(
            Scope::parse::<Ac2>(text).unwrap(),
            Scope::Item(canonical.into())
        );
    }
    assert_eq!(
        Item::parse(&["camp", "c-1@77B8"]).unwrap().landblock(),
        Some(0x77B8)
    );
    assert_eq!(
        Scope::parse::<Ac2>("landblock 77B8").unwrap(),
        Scope::Landblock(0x77B8)
    );
    for bad in [
        "object s-5c",
        "camp Beetles@77B8",
        "loot GP_Beetle",
        "sale 0x6F0016F6",
        "talk",
        "route Guards",
        "area 41A",
        "weenie 1234",
    ] {
        let e = Scope::parse::<Ac2>(bad).unwrap_err();
        assert!(e.contains("talk <key>"), "{bad}: {e}");
    }
}

#[test]
fn an_override_and_a_removal_name_items_in_the_game_s_words() {
    let w = world(
        "key = \"w\"\ngame = \"ac2\"\nera = \"eor\"\npacks = [\"a\", \"b\"]\n\
         [[override]]\npack = \"b\"\nover = \"a\"\nitems = [\"keeper 0x470033D0\", \"landblock 0298\"]\n",
    )
    .unwrap();
    assert_eq!(
        w.overrides[0].items,
        Some(vec![
            Scope::Item("keeper 0x470033d0".into()),
            Scope::Landblock(0x0298)
        ])
    );
    let r = Remove {
        item: "camp c-4586@77B8".into(),
        prov: None,
    };
    let at = Loc::new(Path::new("b/server/fixes.toml"), 3);
    assert_eq!(r.item_name::<Ac2>(&at).unwrap(), "camp c-4586@77B8");
    let header = "name = \"b\"\nversion = \"0.1.0\"\ngame = \"ac2\"\nlicense = \"CC0-1.0\"\n";
    assert_eq!(
        PackHeader::parse::<Ac2>(Path::new("b/pack.toml"), header)
            .unwrap()
            .name,
        "b"
    );
}
