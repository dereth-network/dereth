//! Behaviour: none (tooling: the release command's preconditions and its push commands).

use super::*;

fn clean(current: &str) -> RepoState {
    RepoState {
        branch: "main".to_owned(),
        uncommitted: Vec::new(),
        current: current.to_owned(),
        local_tags: vec!["empyrean-v0.0.9".to_owned()],
        remote_tags: Some(vec!["empyrean-v0.0.9".to_owned()]),
    }
}

/// A new version on a clean main is released, bumping the crates first.
#[test]
fn a_new_version_on_a_clean_main_is_bumped_and_tagged() {
    assert_eq!(
        preconditions("0.2.0", &clean("0.1.0")),
        Ok(Plan {
            bump: true,
            tag: "empyrean-v0.2.0".to_owned()
        })
    );
}

/// The version the crates already carry, never tagged, is released without a bump: the first
/// release of the version the tree was written at.
#[test]
fn the_carried_version_never_tagged_is_released_without_a_bump() {
    assert_eq!(
        preconditions("0.1.0", &clean("0.1.0")),
        Ok(Plan {
            bump: false,
            tag: "empyrean-v0.1.0".to_owned()
        })
    );
}

/// A tag that exists here or on the remote, an older version, another branch, uncommitted
/// changes and a version that is not three numbers are each refused, and every reason is given.
#[test]
fn every_unmet_precondition_is_refused_and_named() {
    let mut tagged = clean("0.1.0");
    tagged.local_tags.push("empyrean-v0.1.0".to_owned());
    let e = preconditions("0.1.0", &tagged).expect_err("tagged here");
    assert!(e.iter().any(|p| p.contains("already exists here")), "{e:?}");

    let mut remote = clean("0.1.0");
    remote.remote_tags = Some(vec!["empyrean-v0.1.0".to_owned()]);
    let e = preconditions("0.1.0", &remote).expect_err("tagged there");
    assert!(e.iter().any(|p| p.contains("on the remote")), "{e:?}");

    let e = preconditions("0.0.9", &clean("0.1.0")).expect_err("older");
    assert!(e.iter().any(|p| p.contains("older")), "{e:?}");

    let mut elsewhere = clean("0.1.0");
    elsewhere.branch = "empyrean-release".to_owned();
    elsewhere.uncommitted = vec![" M empyrean/README.md".to_owned()];
    let e = preconditions("0.2.0", &elsewhere).expect_err("two reasons");
    assert_eq!(e.len(), 2, "{e:?}");
    assert!(
        e[0].contains("`main`") && e[1].contains("empyrean/README.md"),
        "{e:?}"
    );

    assert!(
        preconditions("0.2.0-rc.1", &clean("0.1.0")).is_ok(),
        "a release candidate is a release"
    );
    for bad in ["0.2", "0.2.0+b", "v0.2.0"] {
        assert!(preconditions(bad, &clean("0.1.0")).is_err(), "{bad}");
    }
}

/// With no answer from the remote, the local tags still decide.
#[test]
fn without_the_remote_the_local_tags_still_decide() {
    let mut offline = clean("0.1.0");
    offline.remote_tags = None;
    assert!(preconditions("0.2.0", &offline).is_ok());
    offline.local_tags.push("empyrean-v0.2.0".to_owned());
    assert!(preconditions("0.2.0", &offline).is_err());
}

/// Tag names are read from both `git tag` and `git ls-remote` output, peeled or not.
#[test]
fn tag_names_are_read_from_local_and_remote_listings() {
    assert_eq!(
        tag_names("empyrean-v0.1.0\nempyrean-v0.2.0\n"),
        ["empyrean-v0.1.0", "empyrean-v0.2.0"]
    );
    assert_eq!(
        tag_names("abc\trefs/tags/empyrean-v0.1.0\ndef\trefs/tags/empyrean-v0.1.0^{}\n"),
        ["empyrean-v0.1.0", "empyrean-v0.1.0"]
    );
}

/// The push publishes main and then the tag, to the named remote.
#[test]
fn the_push_publishes_main_then_the_tag() {
    let [first, second] = push_commands("forgejo", "empyrean-v0.2.0");
    assert_eq!(first, ["git", "push", "forgejo", "main"]);
    assert_eq!(second, ["git", "push", "forgejo", "empyrean-v0.2.0"]);
}
