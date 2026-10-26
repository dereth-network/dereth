//! Behaviour: none (tooling: Lyceum's Characters tab: a character made again by Reset, and exported)
//! The top row holds the recipe, Play As, Reset, Delete and Export, and without the editor's own
//! server nothing is made or exported. Live, against the editor's own server: a character made,
//! made again from its recipe by Reset with no backup kept, exported, and imported by a second
//! server on the loopback, whose operator allows it, into a player's account.

use crate::world::dungeon::keep;
use crate::{editor_with, gpu_lock, sample};

#[test]
fn the_top_row_holds_play_as_reset_delete_and_export_and_without_a_server_nothing_is_made() {
    let _g = gpu_lock();
    let (_dir, world) = sample("character-top-row");
    let mut ed = editor_with(&world, &["--size", "1600x1000"]);
    ed.open_characters();
    for _ in 0..3 {
        assert!(ed.frame());
    }
    ed.new_character("Top Row").unwrap();
    for _ in 0..3 {
        assert!(ed.frame());
    }
    let reset = ed.characters.reset_button.expect("Reset");
    let tabs = &ed.characters.tab_rects;
    assert_eq!(tabs.len(), 5, "Build, Tweak, Items, Augments, Quests");
    assert!(reset.max.y < tabs[0].1.min.y, "above the tabs");
    // Without the editor's own server nothing is made, and nothing is exported.
    assert!(ed.reset_character().is_err());
    assert!(
        ed.export_character().is_err(),
        "a character the server has not made"
    );
    keep(&mut ed, "character-files", "01-top-row.png");
}

/// A second world on the loopback, made of the editor's own (its world database, data overlay and
/// content), with its own databases in `dir` and character import allowed; its console's lines.
#[cfg(feature = "live-server-tests")]
struct OtherWorld {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    lines: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

#[cfg(feature = "live-server-tests")]
impl OtherWorld {
    fn start(server: &std::path::Path, live: &std::path::Path, dir: &std::path::Path) -> Self {
        use std::io::BufRead;
        std::fs::create_dir_all(dir).unwrap();
        let port = (0..64)
            .find_map(|_| {
                let s = std::net::UdpSocket::bind("127.0.0.1:0").ok()?;
                let p = s.local_addr().ok()?.port();
                let next = std::net::UdpSocket::bind(("127.0.0.1", p.checked_add(1)?)).ok()?;
                drop((s, next));
                Some(p)
            })
            .expect("a free port");
        let fwd = |p: &std::path::Path| p.display().to_string().replace('\\', "/");
        let config: String = std::fs::read_to_string(live.join("empyrean.toml"))
            .unwrap()
            .lines()
            .map(|l| {
                if l.starts_with("port = ") {
                    format!("port = {port}")
                } else if l.starts_with("shard_db_path = ") {
                    format!("shard_db_path = \"{}/shard.db\"", fwd(dir))
                } else if l.starts_with("auth_db_path = ") {
                    format!("auth_db_path = \"{}/auth.db\"", fwd(dir))
                } else if l == "[server.accounts]" {
                    format!("{l}\nallow_character_import = true")
                } else {
                    l.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(config.contains("allow_character_import = true"), "{config}");
        std::fs::write(dir.join("empyrean.toml"), config).unwrap();
        let mut cmd = std::process::Command::new(server);
        cmd.arg("--config")
            .arg(dir.join("empyrean.toml"))
            .current_dir(dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No console window of its own.
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        for out in [
            child
                .stdout
                .take()
                .map(|o| Box::new(o) as Box<dyn std::io::Read + Send>),
            child
                .stderr
                .take()
                .map(|o| Box::new(o) as Box<dyn std::io::Read + Send>),
        ]
        .into_iter()
        .flatten()
        {
            let lines = std::sync::Arc::clone(&lines);
            std::thread::spawn(move || {
                for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                    lines.lock().unwrap().push(line);
                }
            });
        }
        let mut world = Self {
            child,
            stdin,
            lines,
        };
        world.wait_for("command prompt ready", 0);
        world
    }

    fn mark(&self) -> usize {
        self.lines.lock().unwrap().len()
    }

    /// The first line from line `from` on that holds `needle`, once the server says it.
    fn wait_for(&mut self, needle: &str, from: usize) -> String {
        let start = std::time::Instant::now();
        loop {
            if let Some(l) = self.lines.lock().unwrap()[from..]
                .iter()
                .find(|l| l.contains(needle))
            {
                return l.clone();
            }
            // Only against a hang: the server's line is what ends the wait.
            assert!(
                start.elapsed() < std::time::Duration::from_secs(180)
                    && matches!(self.child.try_wait(), Ok(None)),
                "the other world never said {needle:?}: {:?}",
                self.lines.lock().unwrap()
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    fn say(&mut self, line: &str) {
        use std::io::Write;
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
    }
}

#[cfg(feature = "live-server-tests")]
impl Drop for OtherWorld {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(feature = "live-server-tests")]
#[test]
fn a_character_reset_from_its_recipe_and_exported_is_imported_by_another_world_as_a_players() {
    use std::path::PathBuf;
    let _g = gpu_lock();
    let (dir, world) = sample("live-character-files");
    let server =
        PathBuf::from(std::env::var_os("EMPYREAN_SERVER_BIN").expect("EMPYREAN_SERVER_BIN"));
    let sql = PathBuf::from(std::env::var_os("EMPYREAN_WORLD_SQL").expect("EMPYREAN_WORLD_SQL"));
    let mut ed = editor_with(
        &world,
        &[
            "--empyrean",
            server.parent().unwrap().to_str().unwrap(),
            "--weenies",
            sql.to_str().unwrap(),
            "--size",
            "1600x1000",
        ],
    );
    ed.wait_for_names().unwrap();
    ed.open_characters();
    ed.new_character("Traveler").unwrap();
    ed.characters.recipe.level = 60;
    ed.characters.save().unwrap();
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    let shard = dir.path().join(".playtest").join("world").join("shard.db");
    let level = |id: u32| -> i64 {
        let db = rusqlite::Connection::open(&shard).unwrap();
        db.query_row(
            "SELECT value FROM biota_properties_int WHERE object_Id = ?1 AND type = 25",
            [id],
            |r| r.get(0),
        )
        .unwrap()
    };

    // Made new, then made again by Reset, the second time with its recipe unchanged: made from
    // its recipe each time, and no backup kept.
    ed.make_character().unwrap();
    ed.characters.recipe.level = 61;
    ed.characters.save().unwrap();
    ed.make_character().unwrap();
    let id = ed.characters.made.as_ref().expect("made").id;
    assert_eq!(level(id), 61);
    let mark = ed.live_log(0).len();
    ed.make_character().unwrap();
    assert!(
        ed.live_log(mark)
            .iter()
            .any(|l| l.contains("Playtest character updated.")),
        "made again though its recipe is as it was"
    );
    assert_eq!(level(id), 61);
    assert!(!dir
        .path()
        .join(".playtest")
        .join("character-backups")
        .exists());
    assert!(!ed.characters.needs_making(), "it is its recipe's");

    // Exported to the world's output folder.
    ed.export_character().unwrap();
    ed.wait_character().unwrap();
    let file = dir
        .path()
        .join("build")
        .join("export")
        .join("characters")
        .join("Traveler.empyrean-character");
    assert!(file.is_file(), "{}", ed.characters.message);
    assert!(
        ed.characters.message.starts_with("Exported Traveler to "),
        "{}",
        ed.characters.message
    );
    {
        let db = rusqlite::Connection::open(&file).unwrap();
        let fact = |key: &str| -> String {
            db.query_row(
                "SELECT value FROM character_file WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(fact("world"), "sample-world");
        assert_eq!(fact("era"), "eor");
        assert_eq!(fact("name"), "Traveler");
        assert_eq!(fact("checks"), "", "a character a player could have");
        assert_eq!(fact("hash"), ed.characters.made.as_ref().unwrap().hash);
        assert_eq!(
            fact("recipe"),
            ed.characters.recipe.text().unwrap(),
            "made from it, the character is the recipe's"
        );
        let classes: i64 = db
            .query_row("SELECT COUNT(*) FROM character_file_class", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(classes > 3, "{classes}");
    }
    for _ in 0..3 {
        assert!(ed.frame());
    }
    keep(&mut ed, "character-files", "02-exported.png");

    // Another world imports it into a player's account.
    let other_dir = dir.path().join("other-world");
    let mut other = OtherWorld::start(
        &server,
        &dir.path().join(".playtest").join("world"),
        &other_dir,
    );
    let mark = other.mark();
    other.say("accountcreate traveler pw Player");
    other.wait_for("traveler", mark);
    let mark = other.mark();
    other.say(&format!(
        "import-character \"{}\" traveler",
        file.display().to_string().replace('\\', "/")
    ));
    let answer = other.wait_for("mported", mark);
    assert!(answer.contains("Imported Traveler with "), "{answer}");
    drop(other);
    let db = rusqlite::Connection::open(other_dir.join("shard.db")).unwrap();
    db.create_collation("utf8mb4_uca1400_ai_ci", |a: &str, b: &str| a.cmp(b))
        .unwrap();
    let (new_id, account): (u32, u32) = db
        .query_row(
            "SELECT id, account_Id FROM \"character\" WHERE name = 'Traveler'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(account > 0);
    let weenie_type: i64 = db
        .query_row(
            "SELECT weenie_Type FROM biota WHERE id = ?1",
            [new_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(weenie_type, 10, "a player's account: a player's body");
    let held: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM biota_properties_i_i_d WHERE value = ?1 AND type IN (2, 3)",
            [new_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(held > 0, "what it holds came with it, under its new id");
    drop(db);
    ed.set_live(false).unwrap();
}
