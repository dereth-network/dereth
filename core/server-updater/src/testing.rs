//! A stand-in for a repository's releases, for a server's updater tests: a web server on loopback,
//! and releases published on it as the release packaging lays them out. Nothing here touches the
//! network beyond this machine. Also programs a test writes and then starts ([`write_program`],
//! [`copy_program`]).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::verify::sha256_hex;

/// A web server on loopback answering GET requests from a table of paths; anything else is 404.
#[derive(Debug, Clone)]
pub struct Loopback {
    /// `http://127.0.0.1:<port>`.
    pub base: String,
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    /// Every path asked for, in turn.
    pub asked: Arc<Mutex<Vec<String>>>,
}

impl Loopback {
    /// Starts it, on a thread of its own and a port of its own.
    ///
    /// # Panics
    /// When no loopback port can be had.
    #[must_use]
    pub fn start() -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a loopback port");
        let base = format!("http://{}", listener.local_addr().expect("its address"));
        let files: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::default();
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let (served, seen) = (Arc::clone(&files), Arc::clone(&asked));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let served = Arc::clone(&served);
                let seen = Arc::clone(&seen);
                std::thread::spawn(move || {
                    let Ok(clone) = stream.try_clone() else {
                        return;
                    };
                    let mut reader = BufReader::new(clone);
                    let mut request = String::new();
                    if reader.read_line(&mut request).is_err() {
                        return;
                    }
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                            break;
                        }
                    }
                    let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                    seen.lock().expect("the list").push(path.clone());
                    let body = served.lock().expect("the table").get(&path).cloned();
                    let (status, body) = match body {
                        Some(b) => ("200 OK", b),
                        None => ("404 Not Found", b"not found".to_vec()),
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(&body);
                });
            }
        });
        Self { base, files, asked }
    }

    /// Serves `bytes` at `path`.
    ///
    /// # Panics
    /// When a thread answering a request panicked.
    pub fn put(&self, path: &str, bytes: impl Into<Vec<u8>>) {
        self.files
            .lock()
            .expect("the table")
            .insert(path.to_owned(), bytes.into());
    }
}

/// A `.zip` holding one folder named `root`, with `files` in it, each marked executable.
///
/// # Panics
/// Never for these inputs: the archive is written to memory.
#[must_use]
pub fn zip_archive(root: &str, files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
    zip.add_directory(format!("{root}/"), opts)
        .expect("a folder");
    for (name, bytes) in files {
        zip.start_file(format!("{root}/{name}"), opts)
            .expect("a file");
        zip.write_all(bytes).expect("its bytes");
    }
    zip.finish().expect("the archive").into_inner()
}

/// One release to publish.
#[derive(Debug, Clone)]
pub struct Release<'a> {
    /// Its version (`0.1.1`).
    pub version: &'a str,
    /// What its archive holds, for [`Release::target`].
    pub files: Vec<(String, Vec<u8>)>,
    /// The target its archive is for.
    pub target: &'a str,
    /// Other targets' archives listed before this one's in `release.json`, each holding nothing.
    pub other_targets: &'a [&'a str],
    /// `release.json`'s `upgrade`.
    pub upgrade: Value,
    /// Published as a pre-release.
    pub prerelease: bool,
}

/// A repository's releases of one program, as the releases API and its downloads serve them.
#[derive(Debug, Clone)]
pub struct Repository {
    pub web: Loopback,
    /// The program's name, and what its archives begin with (`yalain`).
    program: String,
    /// What its tags begin with (`yalain-v`).
    tag_prefix: String,
    /// The list so far, newest first.
    list: Arc<Mutex<Vec<Value>>>,
}

impl Repository {
    /// The API address of the stand-in repository, for [`crate::source::Source`].
    #[must_use]
    pub fn api(&self) -> String {
        format!("{}/repos/example/{}", self.web.base, self.program)
    }

    /// A repository of `program`'s releases on a web server of its own.
    #[must_use]
    pub fn start(program: &str, tag_prefix: &str) -> Self {
        let repository = Self {
            web: Loopback::start(),
            program: program.to_owned(),
            tag_prefix: tag_prefix.to_owned(),
            list: Arc::default(),
        };
        repository.put_list();
        repository
    }

    fn put_list(&self) {
        let list = Value::Array(self.list.lock().expect("the list").clone());
        self.web.put(
            &format!("/repos/example/{}/releases?per_page=100", self.program),
            list.to_string(),
        );
    }

    /// Lists a release of another program in the same repository.
    ///
    /// # Panics
    /// When a thread holding the list panicked.
    pub fn list_other(&self, tag: &str) {
        self.list.lock().expect("the list").insert(
            0,
            json!({"tag_name": tag, "draft": false, "prerelease": false, "assets": []}),
        );
        self.put_list();
    }

    /// Publishes `release`, newest first, with its archive, `release.json` and `SHA256SUMS`;
    /// `sums_edit` may alter the sums.
    ///
    /// # Panics
    /// When a thread holding the list panicked.
    pub fn publish(&self, release: &Release<'_>, sums_edit: impl Fn(String) -> String) {
        let version = release.version;
        let root = format!("{}-{version}-{}", self.program, release.target);
        let file = format!("{root}.zip");
        let archive = zip_archive(&root, &release.files);
        let mut assets: Vec<Value> = release
            .other_targets
            .iter()
            .map(|t| {
                json!({
                    "target": t,
                    "file": format!("{}-{version}-{t}.zip", self.program),
                    "size": 1,
                    "sha256": sha256_hex(b"another target's archive"),
                })
            })
            .collect();
        assets.push(json!({
            "target": release.target,
            "file": file,
            "size": archive.len(),
            "sha256": sha256_hex(&archive),
        }));
        let index = json!({
            "schema": 2,
            "product": self.program,
            "version": version,
            "prerelease": release.prerelease,
            "assets": assets,
            "upgrade": release.upgrade,
        })
        .to_string();
        let sums = sums_edit(format!(
            "{}  {file}\n{}  release.json\n",
            sha256_hex(&archive),
            sha256_hex(index.as_bytes())
        ));
        let tag = format!("{}{version}", self.tag_prefix);
        let url = |name: &str| format!("{}/download/{tag}/{name}", self.web.base);
        self.web.put(&format!("/download/{tag}/{file}"), archive);
        self.web
            .put(&format!("/download/{tag}/release.json"), index);
        self.web.put(&format!("/download/{tag}/SHA256SUMS"), sums);
        self.list.lock().expect("the list").insert(
            0,
            json!({"tag_name": tag, "draft": false, "prerelease": release.prerelease, "assets": [
                {"name": file, "browser_download_url": url(&file)},
                {"name": "release.json", "browser_download_url": url("release.json")},
                {"name": "SHA256SUMS", "browser_download_url": url("SHA256SUMS")}
            ]}),
        );
        self.put_list();
    }
}

/// Writes `bytes` as a program at `path` (replacing any file there), for a test that then starts
/// it.
///
/// A test binary's other tests start processes as they please, not through [`crate::program`], and
/// a process started while this one holds the program open for writing could still be holding it
/// when the test starts it, which Linux refuses ("Text file busy"). So on Unix the program is made
/// by a process of its own (`cp`) from the bytes written beside it under another name: this
/// process never holds the program open for writing, and so nothing it starts can.
///
/// # Panics
/// When the program cannot be written.
pub fn write_program(path: &Path, bytes: &[u8]) {
    if cfg!(unix) {
        let mut beside = path.as_os_str().to_owned();
        beside.push(".bytes");
        let beside = std::path::PathBuf::from(beside);
        std::fs::write(&beside, bytes).unwrap_or_else(|e| panic!("{}: {e}", beside.display()));
        copy_program(&beside, path);
        let _ = std::fs::remove_file(&beside);
    } else {
        crate::program::write(path, bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

/// Copies the file `from` as a program to `to` (replacing any file there), for a test that then
/// starts it, as [`write_program`] makes one.
///
/// # Panics
/// When the program cannot be copied.
pub fn copy_program(from: &Path, to: &Path) {
    if cfg!(unix) {
        match std::fs::remove_file(to) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("{}: {e}", to.display()),
        }
        let copied =
            crate::program::status(std::process::Command::new("cp").arg("--").arg(from).arg(to))
                .unwrap_or_else(|e| panic!("cp runs: {e}"));
        assert!(
            copied.success(),
            "cp {} {}: {copied}",
            from.display(),
            to.display()
        );
        crate::program::make_executable(to).unwrap_or_else(|e| panic!("{}: {e}", to.display()));
    } else {
        crate::program::copy(from, to).unwrap_or_else(|e| panic!("{}: {e}", to.display()));
    }
}
