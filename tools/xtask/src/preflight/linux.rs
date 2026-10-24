//! What the preflight runs on a Linux machine for one of its worlds, besides starting and stopping
//! its unit: shell scripts that stop at the first command that fails (`set -e`), one command a
//! line, every path quoted. Files cross with scp.
//!
//! A file of a world is replaced by a copy over it where the account may write it, which keeps
//! its owner and mode. A file root has put there since the setup may leave the account no write
//! (its access list's mask comes from the mode root gave it), so that one is replaced by a rename
//! instead, given its mode first, which needs write access to the folder only.

use super::config::World;

/// `s` as a shell word, quoted.
pub fn sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Where `w`'s files as they were are kept, beside it.
pub fn backup_dir(w: &World) -> String {
    format!("{}/.preflight-backup", w.dir.trim_end_matches('/'))
}

/// Where what replaces `file` is copied up or put first, beside it.
pub fn arriving(file: &str) -> String {
    format!("{file}.preflight-new")
}

/// Where a build is copied up to, beside the server it replaces.
pub fn incoming(w: &World) -> String {
    arriving(&w.replaced())
}

/// Where a configuration is copied up to, beside the one it replaces.
pub fn incoming_config(w: &World) -> String {
    arriving(&w.config_file())
}

/// The test that `file` is there and the account may not write it.
fn locked(file: &str) -> String {
    let f = sh(file);
    format!("[ -e {f} ] && [ ! -w {f} ]")
}

/// The commands that rename `new`, beside `live`, over it, given its mode first.
fn rename_over(new: &str, live: &str) -> String {
    let (n, l) = (sh(new), sh(live));
    format!("chmod --reference {l} {n}; mv -f {n} {l}")
}

/// The line that puts `new`, copied up beside `live`, in its place.
fn put(new: &str, live: &str) -> String {
    format!(
        "if {}; then {}; else cp {n} {l}; rm -f {n}; fi\n",
        locked(live),
        rename_over(new, live),
        n = sh(new),
        l = sh(live)
    )
}

/// The test and branches of an `if` that put `kept`, a backup, back as `live`, the backup left as
/// it is: `<test>; then …; else …`.
fn put_back(kept: &str, live: &str) -> String {
    let beside = arriving(live);
    format!(
        "{}; then cp {k} {}; {}; else cp {k} {}",
        locked(live),
        sh(&beside),
        rename_over(&beside, live),
        sh(live),
        k = sh(kept)
    )
}

/// The name of `file`, without its folder.
fn base(file: &str) -> &str {
    file.rsplit('/').next().unwrap_or(file)
}

/// The database files that go with each of `dbs`: each, its write-ahead log and its shared memory.
fn database_files(dbs: &[String]) -> Vec<String> {
    dbs.iter()
        .flat_map(|db| ["", "-wal", "-shm"].map(|suffix| format!("{db}{suffix}")))
        .collect()
}

/// `w`'s files as they are, kept beside it: its server, its configuration, and the database files
/// `dbs` (with their logs, which may hold what a world stopped after a crash has not yet written
/// to its database) that are there. Each is only read, into a folder the account makes.
pub fn backup(w: &World, dbs: &[String]) -> String {
    let b = backup_dir(w);
    let mut s = format!(
        "set -e\nrm -rf {b_}\nmkdir -p {db}\ncp -a {} {}\ncp -a {} {}\n",
        sh(&w.replaced()),
        sh(&format!("{b}/replaced")),
        sh(&w.config_file()),
        sh(&format!("{b}/config")),
        b_ = sh(&b),
        db = sh(&format!("{b}/db")),
    );
    for f in database_files(dbs) {
        s.push_str(&format!(
            "if [ -e {f_} ]; then cp -a {f_} {}; fi\n",
            sh(&format!("{b}/db/{}", base(&f))),
            f_ = sh(&f)
        ));
    }
    s
}

/// What was copied up beside `w`'s files put in their place: the build over its server (when
/// `build`), and the configuration over its own.
pub fn place(w: &World, build: bool) -> String {
    let mut s = "set -e\n".to_owned();
    if build {
        s.push_str(&put(&incoming(w), &w.replaced()));
    }
    s.push_str(&put(&incoming_config(w), &w.config_file()));
    s
}

/// `w`'s files put back from its backup: its server, its configuration, and each database file,
/// or, when the backup has none, the file taken out (a log made since). A plain copy, not `cp -a`:
/// the files are the service's, and the account may write them but not set their times.
pub fn restore(w: &World, dbs: &[String]) -> String {
    let b = backup_dir(w);
    let mut s = format!(
        "set -e\nif {}; fi\nif {}; fi\n",
        put_back(&format!("{b}/replaced"), &w.replaced()),
        put_back(&format!("{b}/config"), &w.config_file()),
    );
    for f in database_files(dbs) {
        let kept = format!("{b}/db/{}", base(&f));
        s.push_str(&format!(
            "if [ ! -e {} ]; then rm -f {}; elif {}; fi\n",
            sh(&kept),
            sh(&f),
            put_back(&kept, &f)
        ));
    }
    s
}

/// `w`'s backup removed, once the world it put back runs.
pub fn drop_backup(w: &World) -> String {
    format!("set -e\nrm -rf {}\n", sh(&backup_dir(w)))
}
