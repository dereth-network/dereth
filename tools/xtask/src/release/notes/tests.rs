//! Behaviour: none (tooling: which commits a release's notes keep, under which heading, and
//! how the highlights file is read and stamped).

use super::*;

fn commit(subject: &str, paths: &[&str]) -> Commit {
    Commit {
        subject: subject.to_owned(),
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        comment_only: false,
    }
}

/// A commit that changes only Empyrean, CI, the build tools, tests or documents is left out, as
/// is a release's own version commit; one that changes the client, the shared core or the
/// launcher is kept.
#[test]
fn only_commits_that_change_what_the_client_ships_are_kept() {
    let left_out = [
        commit(
            "The server answers a portal use",
            &["empyrean/crates/world/src/portal.rs"],
        ),
        commit("CI runs on branch pushes", &[".github/workflows/ci.yml"]),
        commit(
            "xtask release makes the fixture",
            &["tools/xtask/src/release.rs", "empyrean/RELEASING.md"],
        ),
        commit(
            "render: the Vulkan tests skip without a driver",
            &[
                "dereth/client/tests/gpu/rendering/mips.rs",
                "dereth/testkit/src/behaviours/rendering.rs",
                "core/physics/src/tests.rs",
                "dereth/launcher/src/update_tests.rs",
            ],
        ),
        commit(
            "The launcher's release notes",
            &["dereth/launcher/RELEASING.md", "dereth/CHANGES.md"],
        ),
        commit(
            "Dereth 0.1.1",
            &["dereth/client/Cargo.toml", "dereth/launcher/Cargo.toml"],
        ),
        commit(
            "Empyrean 0.2.0-rc.1: its upgrade fixture",
            &["core/dat/src/lib.rs"],
        ),
    ];
    for c in &left_out {
        assert_eq!(group(Product::Dereth, c), None, "{c:?}");
    }
    let kept = [
        commit("Walking uphill is slower", &["core/physics/src/walk.rs"]),
        commit(
            "A body is drawn whole",
            &[
                "dereth/client/crates/scene/src/world_scene.rs",
                "dereth/client/tests/gpu/rendering/cells.rs",
            ],
        ),
        commit(
            "The launcher remembers its window",
            &["dereth/launcher/src/main.rs"],
        ),
        commit(
            "The web relay keeps the socket open",
            &["tools/web-relay/src/main.rs"],
        ),
    ];
    for c in &kept {
        assert!(group(Product::Dereth, c).is_some(), "{c:?}");
    }
}

/// A launcher-only commit is the launcher's whatever its words; a subject that says it fixes
/// something is a fix; otherwise the one heading the subject's words name wins.
#[test]
fn a_commit_is_grouped_by_the_launcher_then_a_fix_then_its_words() {
    let cases = [
        (
            "launcher: the window remembers its size",
            &["dereth/launcher/src/window.rs"][..],
            Group::Launcher,
        ),
        (
            "The client no longer stops when a texture is missing",
            &["dereth/client/crates/render/src/texture.rs"][..],
            Group::Fixes,
        ),
        (
            "Fix the chat window's scroll",
            &["dereth/client/crates/ui/src/chat.rs"][..],
            Group::Fixes,
        ),
        (
            "An object's particles are drawn at the object in every landblock",
            &[
                "core/animation/src/particles/emitter.rs",
                "dereth/client/crates/scene/src/particles.rs",
            ][..],
            Group::GraphicsSound,
        ),
        (
            "In-world keys do nothing before the player is in the world",
            &["dereth/client/crates/shell/src/input.rs"][..],
            Group::Interface,
        ),
        (
            "A spell's cast is refused while the caster moves",
            &["core/client-runtime/src/world_step.rs"][..],
            Group::Gameplay,
        ),
    ];
    for (subject, paths, want) in cases {
        assert_eq!(
            group(Product::Dereth, &commit(subject, paths)),
            Some(want),
            "{subject}"
        );
    }
}

/// When the words name several headings, the commit's folders choose among them; when the words
/// name none, the folders choose alone; when neither settles one heading, the commit goes under
/// "Other changes" rather than a guess.
#[test]
fn folders_settle_what_the_words_leave_open_and_the_rest_is_other() {
    // Interface (click) and Gameplay (buy) by words; the vendor panel's folder is the interface's.
    let both = commit(
        "A double-click on a shopkeeper's stock row buys it",
        &["dereth/client/crates/ui-screens/src/panels/vendor.rs"],
    );
    assert_eq!(group(Product::Dereth, &both), Some(Group::Interface));
    // No heading by words; the renderer's folder.
    let folder = commit(
        "The frame is presented once per refresh",
        &["dereth/client/crates/render/src/present.rs"],
    );
    assert_eq!(group(Product::Dereth, &folder), Some(Group::GraphicsSound));
    // No heading by words, and folders that imply none.
    let neither = commit(
        "The session keeps its sequence numbers across a reconnect",
        &["core/client-net/src/session.rs"],
    );
    assert_eq!(group(Product::Dereth, &neither), Some(Group::Other));
    // Words name two headings and the folders imply neither of them.
    let split = commit(
        "Sounds of the chat window",
        &["core/client-runtime/src/world_objects.rs"],
    );
    assert_eq!(group(Product::Dereth, &split), Some(Group::Other));
}

/// Empyrean's notes keep a commit that changes the server, or a shared crate the server is built
/// from when nothing only the client ships changes with it; they leave out the client's commits
/// (shared-crate changes made for the client among them), the server's tests and documents, the
/// tools, the client-only crates and the version commits.
#[test]
fn only_commits_that_change_what_the_server_ships_are_kept() {
    let left_out = [
        commit(
            "An object's particles are drawn at the object in every landblock",
            &[
                "core/animation/src/particles/emitter.rs",
                "core/client-runtime/src/world_step.rs",
                "dereth/client/crates/scene/src/particles.rs",
            ],
        ),
        commit("Keys work", &["dereth/client/crates/input/src/a.rs"]),
        commit(
            "The session keeps its sequence numbers",
            &["core/client-net/src/session.rs"],
        ),
        commit(
            "The swap test covers a bow",
            &[
                "empyrean/testkit/tests/all/combat/combat_mode_swap.rs",
                "empyrean/crates/store/tests/fixtures/upgrade/0.1.1/shard.db",
                "empyrean/DIVERGENCES.md",
            ],
        ),
        commit(
            "xtask release empyrean makes the fixture",
            &["tools/xtask/src/release.rs", "empyrean/RELEASING.md"],
        ),
        commit(
            "Empyrean 0.1.2",
            &["empyrean/server/Cargo.toml", "Cargo.lock"],
        ),
    ];
    for c in &left_out {
        assert_eq!(group(Product::Empyrean, c), None, "{c:?}");
    }
    let kept = [
        commit(
            "Swapping weapons in combat leaves the new weapon's combat mode",
            &[
                "empyrean/crates/world/src/world_objects/player_combat.rs",
                "empyrean/testkit/tests/all/combat/combat_mode_swap.rs",
            ],
        ),
        commit(
            "A protocol field is read as the server writes it",
            &["core/protocol/src/messages.rs"],
        ),
        commit(
            "Both sides agree on the message",
            &[
                "core/protocol/src/messages.rs",
                "core/client-net/src/session.rs",
                "empyrean/crates/net/src/send.rs",
            ],
        ),
    ];
    for c in &kept {
        assert!(group(Product::Empyrean, c).is_some(), "{c:?}");
    }
    // The client's notes keep the client's side and leave out the server-only commit.
    assert!(group(Product::Dereth, &left_out[0]).is_some());
    assert_eq!(group(Product::Dereth, &kept[0]), None);
}

/// A server commit goes under Gameplay, World and content, Operations, Fixes or Other changes:
/// a fix by its words first, then the one heading its words name, then its folders.
#[test]
fn a_server_commit_is_grouped_by_a_fix_then_its_words_then_its_folders() {
    let cases = [
        (
            "Swapping weapons in combat leaves the new weapon's combat mode, not peace",
            &["empyrean/crates/world/src/world_objects/player_combat.rs"][..],
            Group::Gameplay,
        ),
        (
            "A generator spawns its creatures when the landblock wakes",
            &["empyrean/crates/world/src/managers/landblock.rs"][..],
            Group::World,
        ),
        (
            "The server updates itself from the newest release",
            &["empyrean/server/src/update/release.rs"][..],
            Group::Operations,
        ),
        (
            "The shard database no longer locks during a save",
            &["empyrean/crates/store/src/shard.rs"][..],
            Group::Fixes,
        ),
        // No heading by words: the folders choose.
        (
            "A pack built by an older version is read as before",
            &["empyrean/crates/content/src/pack/read.rs"][..],
            Group::World,
        ),
        (
            "Values written as before",
            &["empyrean/server/src/config_file.rs"][..],
            Group::Operations,
        ),
        // No heading by words, and folders that imply none.
        (
            "Sequence numbers wrap as before",
            &["empyrean/crates/net/src/sequence.rs"][..],
            Group::Other,
        ),
    ];
    for (subject, paths, want) in cases {
        assert_eq!(
            group(Product::Empyrean, &commit(subject, paths)),
            Some(want),
            "{subject}"
        );
    }
    let groups = grouped(
        Product::Empyrean,
        &[
            commit("Config keys", &["empyrean/server/src/config_file.rs"]),
            commit("Combat swaps", &["empyrean/crates/world/src/a.rs"]),
            commit("Keys work", &["dereth/client/crates/input/src/a.rs"]),
        ],
    );
    assert_eq!(
        groups,
        [
            (Group::Gameplay, vec!["Combat swaps".to_owned()]),
            (Group::Operations, vec!["Config keys".to_owned()]),
        ]
    );
    assert_eq!(
        render(Product::Empyrean, None, &[], "empyrean-v0.1.1", None),
        "## All changes\n\nNo change to the server since empyrean-v0.1.1.\n\n"
    );
    assert_eq!(changes_file(Product::Empyrean), "empyrean/CHANGES.md");
}

/// A subject's area label is dropped and its first letter capitalised; a subject without one is
/// kept as written.
#[test]
fn a_subject_loses_its_area_label_and_starts_with_a_capital() {
    assert_eq!(
        display_subject("client: winit 0.30, and the game's own cursors"),
        "Winit 0.30, and the game's own cursors"
    );
    assert_eq!(
        display_subject("empyrean-server tests: a timeout counts"),
        "A timeout counts"
    );
    assert_eq!(
        display_subject("Doors play their sounds: open and close"),
        "Doors play their sounds: open and close"
    );
}

/// The notes list each heading that has lines, in the fixed order, the lines oldest first, after
/// the highlights and before one compare link; with no highlights there is no Highlights section.
#[test]
fn the_notes_are_highlights_then_grouped_changes_then_one_compare_link() {
    let commits = [
        commit(
            "Second graphics change: draws",
            &["dereth/client/crates/render/src/a.rs"],
        ),
        commit("Keys work", &["dereth/client/crates/input/src/a.rs"]),
        commit("Server only", &["empyrean/server/src/main.rs"]),
        commit(
            "Third: drawn too",
            &["dereth/client/crates/render/src/b.rs"],
        ),
    ];
    let groups = grouped(Product::Dereth, &commits);
    let text = render(
        Product::Dereth,
        Some("- Fire stays at its campfire\n"),
        &groups,
        "dereth-v0.1.1",
        Some("https://example.invalid/compare/dereth-v0.1.1...dereth-v0.2.0"),
    );
    assert_eq!(
        text,
        "## Highlights\n\n- Fire stays at its campfire\n\n## All changes\n\n\
         ### Graphics & sound\n\n- Second graphics change: draws\n- Third: drawn too\n\n\
         ### Interface\n\n- Keys work\n\n\
         **Full changes:** https://example.invalid/compare/dereth-v0.1.1...dereth-v0.2.0\n"
    );
    let bare = render(Product::Dereth, Some("  \n"), &[], "dereth-v0.1.1", None);
    assert_eq!(
        bare,
        "## All changes\n\nNo change to the client or the launcher since dereth-v0.1.1.\n\n"
    );
}

/// A highlight wrapped over several lines becomes one line, since GitHub shows every line break;
/// list items, headings and blank lines stay where they are.
#[test]
fn a_wrapped_highlight_is_one_line_in_the_notes() {
    assert_eq!(
        unwrap_lines("- Fire stays\n  at its campfire.\n- Keys\n\n### Later\n1. One\n   more\n"),
        "- Fire stays at its campfire.\n- Keys\n\n### Later\n1. One more"
    );
}

const CHANGES_MD: &str = "# Dereth changes\n\nIntro.\n\n## Unreleased\n\n- New thing\n- Other \
                          thing\n\n## 0.1.1 (2026-09-01)\n\n- Old thing\n";

/// A section is read by its name, a version section by the version before its date; a section
/// that is not there reads as none.
#[test]
fn a_changes_section_is_read_by_its_name() {
    assert_eq!(
        section(CHANGES_MD, UNRELEASED).as_deref(),
        Some("- New thing\n- Other thing")
    );
    assert_eq!(section(CHANGES_MD, "0.1.1").as_deref(), Some("- Old thing"));
    assert_eq!(section(CHANGES_MD, "0.1.0"), None);
}

/// Stamping turns the Unreleased section into the release's, dated, under a new empty Unreleased
/// section, and leaves the rest of the file as it was; a version already stamped, or a file with no
/// Unreleased section, is refused.
#[test]
fn stamping_dates_the_unreleased_section_and_opens_a_new_one() {
    let stamped = stamp(CHANGES_MD, "0.2.0", "2026-09-30").expect("stamps");
    assert_eq!(
        stamped,
        "# Dereth changes\n\nIntro.\n\n## Unreleased\n\n## 0.2.0 (2026-09-30)\n\n- New thing\n\
         - Other thing\n\n## 0.1.1 (2026-09-01)\n\n- Old thing\n"
    );
    assert_eq!(section(&stamped, UNRELEASED).as_deref(), Some(""));
    assert_eq!(
        section(&stamped, "0.2.0").as_deref(),
        Some("- New thing\n- Other thing")
    );
    assert!(stamp(&stamped, "0.2.0", "2026-10-01")
        .expect_err("twice")
        .contains("already"));
    assert!(stamp("# Dereth changes\n", "0.2.0", "2026-09-30")
        .expect_err("no section")
        .contains("no `## Unreleased`"));
}

/// The stamp's date is the civil date of the day count.
#[test]
fn a_day_count_is_its_civil_date() {
    assert_eq!(date_of_day(0), "1970-01-01");
    assert_eq!(date_of_day(19_723), "2024-01-01");
    assert_eq!(date_of_day(19_723 + 59), "2024-02-29");
    assert_eq!(date_of_day(20_726), "2026-09-30");
}

/// The command takes either product and its three options, and refuses anything else.
#[test]
fn the_command_takes_a_product_and_its_options() {
    let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        parse(&args(&[
            "dereth",
            "--since",
            "dereth-v0.1.0",
            "--version",
            "0.2.0",
            "--out",
            "n.md"
        ])),
        Ok((
            Product::Dereth,
            Request {
                since: Some("dereth-v0.1.0".to_owned()),
                version: Some("0.2.0".to_owned()),
                out: Some(PathBuf::from("n.md")),
            }
        ))
    );
    assert_eq!(
        parse(&args(&["dereth"])),
        Ok((Product::Dereth, Request::default()))
    );
    assert_eq!(
        parse(&args(&["empyrean", "--version", "0.1.2"])),
        Ok((
            Product::Empyrean,
            Request {
                version: Some("0.1.2".to_owned()),
                ..Request::default()
            }
        ))
    );
    assert!(parse(&args(&["server"])).is_err());
    assert!(parse(&args(&[])).is_err());
    assert!(parse(&args(&["dereth", "--bogus"])).is_err());
    assert!(parse(&args(&["dereth", "--since"])).is_err());
}

/// A diff that changes only `//`, `///` or `//!` lines and blank lines of Rust files is
/// comment-only; a code line, a change to another kind of file, a binary file or a diff with no
/// changed line is not.
#[test]
fn a_diff_of_only_comments_and_blank_lines_is_comment_only() {
    let header = "diff --git a/core/x/src/a.rs b/core/x/src/a.rs\n--- a/core/x/src/a.rs\n\
                  +++ b/core/x/src/a.rs\n@@ -1 +1,2 @@\n";
    let comments =
        format!("{header}-/// Old words.\n+/// New words.\n+\n+    //! Module.\n+// x\n");
    assert!(comment_only(&comments));
    let code = format!("{header}-/// Old.\n+/// New.\n+fn f() {{}}\n");
    assert!(!comment_only(&code));
    let toml = "diff --git a/core/x/Cargo.toml b/core/x/Cargo.toml\n--- a/core/x/Cargo.toml\n\
                +++ b/core/x/Cargo.toml\n@@ -1 +1 @@\n-# a\n+# b\n";
    assert!(!comment_only(&format!("{comments}{toml}")));
    let binary = "diff --git a/core/x/a.png b/core/x/a.png\nBinary files a/core/x/a.png and \
                  b/core/x/a.png differ\n";
    assert!(!comment_only(binary));
    assert!(!comment_only(""));
}

/// A commit whose shipped files change only in comments is left out of the notes.
#[test]
fn a_comment_only_commit_is_left_out() {
    let mut c = commit(
        "client: the window module's doc links name their items by full path",
        &["dereth/client/src/platform/window.rs"],
    );
    assert!(group(Product::Dereth, &c).is_some());
    c.comment_only = true;
    assert_eq!(group(Product::Dereth, &c), None);
}

/// A scratch git repository in the temporary directory, removed when dropped.
struct ScratchRepo(PathBuf);

impl ScratchRepo {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("xtask-notes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dereth")).expect("scratch dir");
        let repo = Self(dir);
        repo.git(&["init", "-q", "-b", "main"]);
        repo
    }

    fn git(&self, args: &[&str]) {
        let mut all = vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
        ];
        all.extend_from_slice(args);
        git(&self.0, &all).expect("git");
    }

    /// Commits `subject`, writing `content` to `path` (relative to the repository).
    fn commit(&self, subject: &str, path: &str, content: &str) {
        let file = self.0.join(path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
        std::fs::write(&file, content).expect("write");
        self.git(&["add", "--", path]);
        self.git(&["commit", "-q", "-m", subject]);
    }
}

impl Drop for ScratchRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A final release's notes cover every change since the previous final release, passing over a
/// pre-release tag between them in the range and the compare link; a pre-release's notes show the
/// Unreleased highlights as its tag has them.
#[test]
fn a_final_release_covers_everything_since_the_previous_final_one() {
    let repo = ScratchRepo::new("final");
    let changes = "# Dereth changes\n\n## Unreleased\n\n- Fire stays at its campfire\n";
    repo.commit("First", CHANGES, changes);
    repo.git(&["tag", "dereth-v0.1.0"]);
    repo.commit(
        "Particles are drawn",
        "core/animation/src/a.rs",
        "fn a() {}\n",
    );
    repo.git(&["tag", "dereth-v0.2.0-rc.1"]);
    repo.commit(
        "Keys work",
        "dereth/client/crates/input/src/a.rs",
        "fn b() {}\n",
    );
    repo.commit(
        "Comments only",
        "dereth/client/crates/input/src/a.rs",
        "/// B.\nfn b() {}\n",
    );
    let stamped = stamp(changes, "0.2.0", "2026-09-30").expect("stamps");
    repo.commit("Dereth 0.2.0", CHANGES, &stamped);
    repo.git(&["tag", "dereth-v0.2.0"]);

    let request = |v: &str| Request {
        version: Some(v.to_owned()),
        ..Request::default()
    };
    let final_notes = notes(&repo.0, Product::Dereth, &request("0.2.0")).expect("final notes");
    assert!(final_notes.starts_with("## Highlights\n\n- Fire stays at its campfire\n"));
    assert!(
        final_notes.contains("- Particles are drawn\n"),
        "{final_notes}"
    );
    assert!(final_notes.contains("- Keys work\n"), "{final_notes}");
    assert!(!final_notes.contains("Comments only"), "{final_notes}");
    assert!(!final_notes.contains("Dereth 0.2.0"), "{final_notes}");
    assert!(
        final_notes.contains("/compare/dereth-v0.1.0...dereth-v0.2.0"),
        "{final_notes}"
    );

    let rc_notes = notes(&repo.0, Product::Dereth, &request("0.2.0-rc.1")).expect("rc notes");
    assert!(rc_notes.starts_with("## Highlights\n\n- Fire stays at its campfire\n"));
    assert!(rc_notes.contains("- Particles are drawn\n"), "{rc_notes}");
    assert!(!rc_notes.contains("Keys work"), "{rc_notes}");
    assert!(
        rc_notes.contains("/compare/dereth-v0.1.0...dereth-v0.2.0-rc.1"),
        "{rc_notes}"
    );
}

/// A commit that only restructures the code (shares, moves, splits or merges it, removes dead
/// code, or renames and documents it) is left out; one whose subject says what the product now
/// does is kept, even when it starts with a verb.
#[test]
fn a_commit_that_only_restructures_the_code_is_left_out() {
    let client = ["dereth/client/src/app.rs"];
    for subject in [
        "Share trade acceptance controls between interfaces",
        "Split the gameplay screen into modules by concern",
        "Move collision tests into a dedicated module",
        "Remove unused model convenience APIs",
        "Remove the unused rendering exception filter",
        "Use owning crates throughout client consumers",
        "Limit product dependencies to runtime ownership",
        "Use consistent interface and format vocabulary",
        "Refresh behavioral documentation and guard source citations",
        "Dispatch registered commands through typed handler meanings",
        "Allow headless desktop input replay",
        "Route classic casting and shortcut creation through shared rules",
    ] {
        assert_eq!(
            group(Product::Dereth, &commit(subject, &client)),
            None,
            "{subject}"
        );
    }
    for subject in [
        "Use Mouse Turning Settings sets the mouse-turning preset",
        "Use the selected classic pack as the pickup destination",
        "Correct Classic drag feedback and adaptive panel layout",
        "The classic doll takes trinkets and aetheria",
        "Remove unsupported local chat clearing",
    ] {
        assert!(
            group(Product::Dereth, &commit(subject, &client)).is_some(),
            "{subject}"
        );
    }
}
