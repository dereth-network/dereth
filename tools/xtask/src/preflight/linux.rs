//! What the preflight runs on a Linux machine for one of its worlds, besides starting and stopping
//! its unit: shell scripts that stop at the first command that fails (`set -e`), one command a
//! line, every path quoted. Files cross with scp.

use super::config::World;

/// `s` as a shell word, quoted.
pub fn sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Where `w`'s files as they were are kept, beside it.
pub fn backup_dir(w: &World) -> String {
    format!("{}/.preflight-backup", w.dir.trim_end_matches('/'))
}

/// Where a build is copied up to, beside the server it replaces.
pub fn incoming(w: &World) -> String {
    format!("{}.preflight-new", w.replaced())
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
/// to its database) that are there.
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

/// The build copied up beside `w`'s server copied over it, which keeps the server's own mode.
pub fn place(w: &World) -> String {
    let new = sh(&incoming(w));
    format!("set -e\ncp {new} {}\nrm -f {new}\n", sh(&w.replaced()))
}

/// `w`'s files put back from its backup: its server, its configuration, and each database file,
/// or, when the backup has none, the file taken out (a log made since).
pub fn restore(w: &World, dbs: &[String]) -> String {
    let b = backup_dir(w);
    let mut s = format!(
        "set -e\ncp -a {} {}\ncp -a {} {}\n",
        sh(&format!("{b}/replaced")),
        sh(&w.replaced()),
        sh(&format!("{b}/config")),
        sh(&w.config_file()),
    );
    for f in database_files(dbs) {
        let kept = sh(&format!("{b}/db/{}", base(&f)));
        s.push_str(&format!(
            "if [ -e {kept} ]; then cp -a {kept} {f_}; else rm -f {f_}; fi\n",
            f_ = sh(&f)
        ));
    }
    s
}

/// `w`'s backup removed, once the world it put back runs.
pub fn drop_backup(w: &World) -> String {
    format!("set -e\nrm -rf {}\n", sh(&backup_dir(w)))
}
