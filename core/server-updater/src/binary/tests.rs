//! Behaviour: none (the updater of a product whose release is one binary: which release its policy
//! takes, the archives' names, reading the binary out of one, the checksums, and the releases API
//! read a page at a time; checked with a stand-in product and fetcher)

use super::*;
use crate::source::{next_page, parse_releases, MAX_PAGES};
use crate::verify::{check_sum, parse_sums, sha256_hex};

/// A product like the hub, with names of its own.
#[derive(Debug)]
struct Example;

impl Product for Example {
    const PROGRAM: &'static str = "example";
    const TAG_PREFIX: &'static str = "example-v";
    const COMMAND: &'static str = "self-update";
    const POLICY_OPTION: &'static str = "--policy";
    const STATE_FILE: &'static str = ".example-update.json";
    const WRITE_CHECK_FILE: &'static str = ".example.write-check";

    fn accept(&self, _: &dyn Runner, _: &Path, _: &Version, _: Policy) -> Result<(), Refusal> {
        Ok(())
    }

    fn back_up(
        &self,
        _: &Version,
        _: &Version,
        _: &mut dyn FnMut(&str),
    ) -> Result<PathBuf, String> {
        Ok(PathBuf::new())
    }
}

fn v(s: &str) -> Version {
    Version::parse(s).expect("a version")
}

fn release(version: &str, prerelease: bool) -> Published {
    let ver = v(version);
    let mut assets = vec![("SHA256SUMS".to_owned(), format!("https://x/{version}/sums"))];
    for t in ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"] {
        let a = archive_name(Example::PROGRAM, &ver, t);
        assets.push((a.clone(), format!("https://x/{version}/{a}")));
    }
    Published {
        version: ver,
        prerelease,
        assets,
    }
}

const LINUX: Option<&str> = Some("x86_64-unknown-linux-gnu");

/// The newest release the policy allows is taken: `patch` this build's major and minor version,
/// `minor` its major version; a major release, a pre-release, one that failed here before, and one
/// without this target's archive or `SHA256SUMS` are reported and never installed.
#[test]
fn the_policy_takes_the_newest_release_it_allows_and_reports_the_rest() {
    let current = v("0.1.1");
    let found = vec![
        release("0.1.0", false),
        release("0.1.2", false),
        release("0.1.3", false),
        release("0.1.4-rc.1", false),
        release("0.1.5", true),
        release("0.2.0", false),
        release("1.0.0", false),
    ];
    let patch = select::<Example>(&current, LINUX, Policy::Patch, &[], &found);
    assert_eq!(patch.take, Some(v("0.1.3")));
    let why = |s: &Selection, version: &str| {
        s.reported
            .iter()
            .find(|(x, _)| *x == v(version))
            .map(|(_, w)| w.clone())
            .unwrap_or_else(|| panic!("{version} is reported"))
    };
    assert!(why(&patch, "0.1.2").contains("0.1.3 is newer"));
    assert!(why(&patch, "0.1.4-rc.1").contains("a pre-release: self-update never installs one"));
    assert!(
        why(&patch, "0.1.5").contains("pre-release"),
        "published as one"
    );
    assert!(why(&patch, "0.2.0").contains("--policy patch takes only 0.1.x"));
    assert!(why(&patch, "1.0.0").contains("a major release: self-update never installs one"));
    assert!(
        !patch.reported.iter().any(|(x, _)| *x == v("0.1.0")),
        "an older release is not a candidate"
    );

    let minor = select::<Example>(&current, LINUX, Policy::Minor, &[], &found);
    assert_eq!(minor.take, Some(v("0.2.0")));
    assert!(why(&minor, "1.0.0").contains("a major release"));

    let failed = [Failure {
        version: "0.2.0".into(),
        reason: "it does not run here".into(),
    }];
    let minor = select::<Example>(&current, LINUX, Policy::Minor, &failed, &found);
    assert_eq!(minor.take, Some(v("0.1.3")), "the one before it instead");
    assert!(why(&minor, "0.2.0").contains("failed here before: it does not run here"));

    let mut no_archive = release("0.1.6", false);
    no_archive
        .assets
        .retain(|(n, _)| !n.contains("linux") || n == "SHA256SUMS");
    let mut no_sums = release("0.1.7", false);
    no_sums.assets.retain(|(n, _)| n != "SHA256SUMS");
    let s = select::<Example>(
        &current,
        LINUX,
        Policy::Patch,
        &[],
        &[release("0.1.2", false), no_archive, no_sums],
    );
    assert_eq!(s.take, Some(v("0.1.2")));
    assert!(why(&s, "0.1.6").contains("publishes no example-0.1.6-x86_64-unknown-linux-gnu.tar.gz"));
    assert!(why(&s, "0.1.7").contains("no SHA256SUMS"));

    assert_eq!(
        select::<Example>(&v("1.0.0"), LINUX, Policy::Minor, &[], &found).take,
        None,
        "nothing newer"
    );
}

/// Each target's release archive is `<program>-<version>-<target>`, a `.zip` on Windows and a
/// `.tar.gz` elsewhere, holding the binary in a folder of that name; a binary built for a target no
/// archive is built for finds none.
#[test]
fn each_target_has_its_archive_name() {
    let ver = v("0.1.2");
    for ((arch, os, env), archive, binary) in [
        (
            ("x86_64", "linux", "gnu"),
            "example-0.1.2-x86_64-unknown-linux-gnu.tar.gz",
            "example",
        ),
        (
            ("aarch64", "linux", "gnu"),
            "example-0.1.2-aarch64-unknown-linux-gnu.tar.gz",
            "example",
        ),
        (
            ("x86_64", "windows", "msvc"),
            "example-0.1.2-x86_64-pc-windows-msvc.zip",
            "example.exe",
        ),
        (
            ("aarch64", "macos", ""),
            "example-0.1.2-aarch64-apple-darwin.tar.gz",
            "example",
        ),
    ] {
        let target = target_for(arch, os, env).expect("a target");
        assert_eq!(archive_name("example", &ver, target), archive, "{target}");
        assert_eq!(binary_name("example", target), binary, "{target}");
    }
    for (arch, os, env) in [
        ("x86_64", "linux", "musl"),
        ("x86_64", "windows", "gnu"),
        ("x86", "windows", "msvc"),
        ("riscv64", "linux", "gnu"),
        ("x86_64", "freebsd", ""),
    ] {
        assert_eq!(target_for(arch, os, env), None, "{arch}-{os}-{env}");
    }
    assert!(this_target().is_some(), "this build's target has archives");
}

/// A gzipped tar of `entries`: (path, type flag, contents).
fn tar_gz(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut tar = Vec::new();
    for (path, kind, data) in entries {
        let mut h = [0u8; 512];
        h[..path.len()].copy_from_slice(path.as_bytes());
        h[100..107].copy_from_slice(b"0000755");
        h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
        h[156] = *kind;
        h[257..263].copy_from_slice(b"ustar\0");
        tar.extend_from_slice(&h);
        tar.extend_from_slice(data);
        tar.resize(tar.len().div_ceil(512) * 512, 0);
    }
    tar.resize(tar.len() + 1024, 0);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&tar).expect("written");
    gz.finish().expect("finished")
}

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, data) in entries {
        if let Some(dir) = path.strip_suffix('/') {
            z.add_directory(dir, zip::write::SimpleFileOptions::default())
                .expect("a folder");
        } else {
            z.start_file(*path, zip::write::SimpleFileOptions::default())
                .expect("a file");
            z.write_all(data).expect("written");
        }
    }
    z.finish().expect("finished").into_inner()
}

/// The binary is read from its folder in the archive, a `.tar.gz` (with a pax header, a GNU long
/// name, or neither) or a `.zip`, and an archive without it, or with it as a link, is refused.
#[test]
fn the_binary_is_read_from_either_kind_of_archive() {
    let linux = "x86_64-unknown-linux-gnu";
    let name = archive_name("example", &v("0.1.2"), linux);
    let root = "example-0.1.2-x86_64-unknown-linux-gnu";
    let bin = format!("{root}/example");
    let long = format!("{root}/{}", "a-licence-with-a-long-name-".repeat(4));
    let good = tar_gz(&[
        (&format!("{root}/"), b'5', b""),
        (&format!("{root}/LICENSE"), b'0', b"licence"),
        ("PaxHeader", b'x', b"30 mtime=1700000000.000000000\n"),
        ("././@LongLink", b'L', long.as_bytes()),
        ("cut short", b'0', b"the long one"),
        (&bin, b'0', b"the binary"),
    ]);
    assert_eq!(
        binary_from_archive(&name, &good, "example").expect("read"),
        b"the binary"
    );
    let link = tar_gz(&[(&bin, b'2', b"")]);
    assert!(binary_from_archive(&name, &link, "example")
        .expect_err("a link")
        .contains("not a plain file"));
    let elsewhere = tar_gz(&[(&format!("{root}/README.md"), b'0', b"guide")]);
    assert!(binary_from_archive(&name, &elsewhere, "example")
        .expect_err("not in its folder")
        .contains("holds no"));
    let loose = tar_gz(&[("example", b'0', b"loose")]);
    assert!(binary_from_archive(&name, &loose, "example")
        .expect_err("outside its folder")
        .contains("refused"));
    assert!(binary_from_archive(&name, b"not gzip", "example").is_err());

    let windows = "x86_64-pc-windows-msvc";
    let name = archive_name("example", &v("0.1.2"), windows);
    let root = "example-0.1.2-x86_64-pc-windows-msvc";
    let good = zip_of(&[
        (&format!("{root}/"), b""),
        (&format!("{root}/README.md"), b"guide"),
        (&format!("{root}/example.exe"), b"the exe"),
    ]);
    assert_eq!(
        binary_from_archive(&name, &good, "example.exe").expect("read"),
        b"the exe"
    );
    let missing = zip_of(&[(&format!("{root}/README.md"), b"guide")]);
    assert!(binary_from_archive(&name, &missing, "example.exe")
        .expect_err("no binary")
        .contains("holds no"));
}

/// A download is checked against `SHA256SUMS`, and one whose SHA-256 is not the one listed, or
/// that is not listed, is refused.
#[test]
fn a_download_must_match_its_listed_sha256() {
    let sum = sha256_hex(b"archive");
    let sums = parse_sums(&format!(
        "{sum}  a.tar.gz\n{}  *b.zip\nnot a line\n",
        sha256_hex(b"other")
    ));
    assert_eq!(sums.len(), 2);
    assert_eq!(check_sum("a.tar.gz", b"archive", &sums), Ok(sum));
    let wrong = check_sum("a.tar.gz", b"altered", &sums).expect_err("refused");
    assert!(wrong.contains("is not the"), "{wrong}");
    assert!(
        check_sum("b.zip", b"other", &sums).is_ok(),
        "a binary-mode line"
    );
    assert!(check_sum("c.tar.gz", b"archive", &sums)
        .expect_err("unlisted")
        .contains("not listed"));
}

/// Releases are the source's tags of the product's prefix, drafts left out, and the source is a
/// GitHub repository or a mirror of its releases API.
#[test]
fn releases_are_the_sources_tags_of_the_products_prefix() {
    let json = r#"[
        {"tag_name": "example-v0.1.2", "prerelease": false, "assets": [
            {"name": "SHA256SUMS", "browser_download_url": "https://dl/sums"}]},
        {"tag_name": "example-v0.2.0-rc.1", "prerelease": true, "assets": []},
        {"tag_name": "example-v0.3.0", "draft": true, "assets": []},
        {"tag_name": "empyrean-v0.4.0", "assets": []},
        {"tag_name": "example-vnext", "assets": []}
    ]"#;
    let found = parse_releases(Example::TAG_PREFIX, json).expect("read");
    assert_eq!(
        found
            .iter()
            .map(|p| p.version.to_string())
            .collect::<Vec<_>>(),
        ["0.1.2", "0.2.0-rc.1"]
    );
    assert_eq!(found[0].asset("SHA256SUMS"), Some("https://dl/sums"));
    assert!(found[1].prerelease);
    assert!(parse_releases(Example::TAG_PREFIX, "{\"message\": \"Not Found\"}").is_err());

    let s = Source::parse("https://github.com/dereth-network/dereth.git/").expect("GitHub");
    assert_eq!(
        (s.api.as_str(), s.repo.as_str()),
        (
            "https://api.github.com/repos/dereth-network/dereth",
            "dereth-network/dereth"
        )
    );
    let m = Source::parse("http://127.0.0.1:9/mirror/repos/me/hub").expect("a mirror");
    assert_eq!(m.api, "http://127.0.0.1:9/mirror/repos/me/hub");
    for bad in [
        "https://github.com/dereth-network",
        "https://gitlab.com/a/b",
        "ftp://x/repos/a/b",
        "",
    ] {
        assert!(Source::parse(bad).is_err(), "{bad}");
    }
    assert!(v("0.1.2-rc.1") < v("0.1.2") && v("0.1.2") < v("0.1.10"));
    assert!(Version::parse("01.2.3").is_err() && Version::parse("1.2").is_err());
}

/// The releases API's list served a page at a time, each page naming the next in its `Link`
/// header as GitHub does, and the pages asked for.
#[derive(Debug)]
struct Pages {
    pages: Vec<Result<Vec<&'static str>, String>>,
    asked: std::cell::RefCell<Vec<usize>>,
    /// Where the next pages are said to be.
    next_at: &'static str,
}

impl Pages {
    fn new(pages: Vec<Vec<&'static str>>) -> Self {
        Self {
            pages: pages.into_iter().map(Ok).collect(),
            asked: std::cell::RefCell::default(),
            next_at: "https://api.github.com/repositories/1/releases?per_page=100",
        }
    }
}

impl Fetcher for Pages {
    fn get(&self, url: &str, _: Fetching) -> Result<Vec<u8>, String> {
        Err(format!("{url} is read as a page of the list"))
    }

    fn list_page(&self, url: &str) -> Result<(Vec<u8>, Option<String>), String> {
        let n = url
            .rsplit_once("&page=")
            .map_or(1, |(_, n)| n.parse::<usize>().expect("a page"));
        self.asked.borrow_mut().push(n);
        let tags = self.pages[n - 1].clone()?;
        let body = format!(
            "[{}]",
            tags.iter()
                .map(|t| format!(r#"{{"tag_name": "{t}", "assets": []}}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        let link = format!(
            "<{}&page={}>; rel=\"next\", <{}&page={}>; rel=\"last\"",
            self.next_at,
            n + 1,
            self.next_at,
            self.pages.len()
        );
        let next = (n < self.pages.len()).then(|| next_page(&link)).flatten();
        Ok((body.into_bytes(), next))
    }
}

fn github() -> Source {
    Source::parse("https://github.com/dereth-network/dereth").expect("GitHub")
}

fn versions(found: &[Published]) -> Vec<String> {
    found.iter().map(|p| p.version.to_string()).collect()
}

fn list(pages: &Pages, current: &str) -> Result<Vec<Published>, String> {
    list_releases(pages, &github(), Example::TAG_PREFIX, &v(current))
}

/// The releases API's `Link` header names the next page of the list.
#[test]
fn the_link_header_names_the_next_page() {
    let at = "https://api.github.com/repositories/1/releases?per_page=100";
    assert_eq!(
        next_page(&format!(
            "<{at}&page=2>; rel=\"next\", <{at}&page=5>; rel=\"last\""
        ))
        .as_deref(),
        Some(format!("{at}&page=2").as_str())
    );
    assert_eq!(
        next_page(&format!(
            "<{at}&page=4>; rel=\"prev\", <{at}&page=1>; rel=\"first\""
        )),
        None,
        "the last page"
    );
    assert_eq!(
        next_page("<https://mirror.test/r?page=2>; rel=next").as_deref(),
        Some("https://mirror.test/r?page=2")
    );
    assert_eq!(next_page(""), None);
}

/// The list is read a page at a time, down to the first page holding a release at or below this
/// build's version, and no further: the newer ones are all above it.
#[test]
fn the_list_is_read_down_to_a_release_at_or_below_this_build() {
    let pages = Pages::new(vec![
        vec!["empyrean-v0.3.5", "example-v0.4.0", "dereth-v0.4.6"],
        vec!["lyceum-v0.1.0", "example-v0.3.2"],
        vec!["empyrean-v0.3.4", "example-v0.3.1", "example-v0.3.0"],
        vec!["example-v0.2.0"],
    ]);
    let found = list(&pages, "0.3.1").expect("listed");
    assert_eq!(*pages.asked.borrow(), [1, 2, 3]);
    assert_eq!(versions(&found), ["0.4.0", "0.3.2", "0.3.1", "0.3.0"]);

    // A build whose own version was never released stops below it all the same.
    let pages = Pages::new(vec![
        vec!["example-v0.4.0"],
        vec!["example-v0.3.2-rc.1"],
        vec!["example-v0.3.1"],
    ]);
    list(&pages, "0.3.2-rc.2").expect("listed");
    assert_eq!(*pages.asked.borrow(), [1, 2]);

    // The newest is on the first page: one page, as before.
    let pages = Pages::new(vec![vec!["example-v0.3.1"], vec!["example-v0.3.0"]]);
    list(&pages, "0.3.1").expect("listed");
    assert_eq!(*pages.asked.borrow(), [1]);
}

/// A newer release a hundred other releases have pushed off the first page is still found, and
/// the reading stops after ten pages.
#[test]
fn a_release_on_a_later_page_is_found_and_ten_pages_are_the_most_read() {
    let mut pages = vec![vec!["empyrean-v0.3.5"]; 4];
    pages.push(vec!["example-v0.3.2", "example-v0.3.1"]);
    let pages = Pages::new(pages);
    let found = list(&pages, "0.3.1").expect("listed");
    assert_eq!(*pages.asked.borrow(), [1, 2, 3, 4, 5]);
    assert_eq!(versions(&found), ["0.3.2", "0.3.1"]);

    let pages = Pages::new(vec![vec!["empyrean-v0.3.5"]; MAX_PAGES + 3]);
    let found = list(&pages, "0.3.1").expect("listed");
    assert_eq!(*pages.asked.borrow(), (1..=MAX_PAGES).collect::<Vec<_>>());
    assert!(found.is_empty());
}

/// A page refused partway (GitHub's hourly limit) fails the check, as a refused first page does,
/// and a next page on another host is not read.
#[test]
fn a_page_refused_partway_fails_the_check_and_another_host_is_not_read() {
    let mut pages = Pages::new(vec![
        vec!["empyrean-v0.3.5"],
        vec!["example-v0.4.0"],
        vec!["example-v0.3.1"],
    ]);
    pages.pages[1] = Err("https://api.github.com/... refused the request (HTTP 403)".into());
    let e = list(&pages, "0.3.1").expect_err("refused");
    assert!(e.contains("HTTP 403"), "{e}");
    assert_eq!(*pages.asked.borrow(), [1, 2]);

    let mut pages = Pages::new(vec![vec!["empyrean-v0.3.5"], vec!["example-v0.4.0"]]);
    pages.next_at = "https://elsewhere.test/releases?per_page=100";
    let found = list(&pages, "0.3.1").expect("listed");
    assert_eq!(*pages.asked.borrow(), [1]);
    assert!(found.is_empty());
}
