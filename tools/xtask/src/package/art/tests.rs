//! Behaviour: none (tooling: Dereth's art pack source, listed from a folder of packed packs).
use super::*;
use dereth_art_catalog::Catalog;

/// A packed pack holding only its manifest, `manifest`, packed from a folder whose hash is
/// `hash`: what a launcher reads of a packed file without its pictures.
pub(crate) fn packed_bytes(manifest: &str, hash: &str) -> Vec<u8> {
    let files = [(
        dereth_assets::art_pack::MANIFEST_FILE.to_owned(),
        manifest.as_bytes().to_vec(),
    )]
    .into();
    dereth_assets::packed_art::pack_files("Test", hash, &files).expect("packed")
}

fn manifest(extra: &str) -> String {
    format!("[pack]\nname = \"Houses\"\nformat = 1\n{extra}")
}

/// A folder of its own for one test, made afresh.
fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-art-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("made");
    dir
}

fn put(dir: &Path, name: &str, bytes: &[u8]) {
    std::fs::write(dir.join(name), bytes).expect("written");
}

/// The packs a folder holds are listed with each file beside the list at a name carrying its id,
/// version and the start of its SHA-256; a file the config does not name is held back; the
/// source's word fills the id and version a pack's manifest leaves out.
#[test]
fn the_named_packs_are_listed_under_hashed_names_and_the_rest_held_back() {
    let dir = folder("listed");
    let houses = packed_bytes(
        &manifest("id = \"com.example.houses\"\nversion = \"1.2.0\"\nlicense = \"CC0-1.0\"\n"),
        "aaaa",
    );
    let old = packed_bytes(&manifest(""), "bbbb");
    put(&dir, "houses.dat", &houses);
    put(&dir, "old.dat", &old);
    put(&dir, "draft.dat", &old);
    put(&dir, "notes.txt", b"not a pack");
    put(
        &dir,
        CONFIG,
        b"name = \"Example\"\n\n[[pack]]\nfile = \"houses.dat\"\nnotes = \"- Roofs.\"\nreleased = \"2026-11-08\"\n\n\
          [[pack]]\nfile = \"old.dat\"\nid = \"com.example.old\"\nversion = \"0.1.0\"\nauthors = [\"Ann\"]\n",
    );
    let source = plan(&dir, None).expect("the source");
    assert_eq!(source.held_back, ["draft.dat"]);
    assert!(source.upload.is_empty());
    let read = Catalog::parse(source.list.as_bytes()).expect("reads");
    assert!(read.skipped.is_empty(), "{:?}", read.skipped);
    assert_eq!(read.catalog.name, "Example");
    let (_, sha_houses) = hash_file(&dir.join("houses.dat")).expect("hashed");
    let h = read.catalog.get("com.example.houses").expect("listed");
    assert_eq!(
        h.file,
        format!(
            "com.example.houses/com.example.houses-1.2.0-{}.dat",
            &sha_houses[..12]
        )
    );
    assert_eq!(
        (h.size, h.sha256.as_str()),
        (houses.len() as u64, sha_houses.as_str())
    );
    assert_eq!(
        (h.license.as_str(), h.notes.as_str()),
        ("CC0-1.0", "- Roofs.")
    );
    assert_eq!(h.released, "2026-11-08");
    assert_eq!(h.hash, "aaaa");
    let o = read
        .catalog
        .get("com.example.old")
        .expect("by the source's word");
    assert_eq!(
        (o.version.as_str(), o.authors.clone()),
        ("0.1.0", vec!["Ann".to_owned()])
    );
    assert_eq!(
        o.released.len(),
        10,
        "dated by the file when the config does not say"
    );
    let mut paths: Vec<&str> = source.site.iter().map(|p| p.path.as_str()).collect();
    paths.sort_unstable();
    let mut files = [o.file.as_str(), h.file.as_str()];
    files.sort_unstable();
    assert_eq!(paths, files);

    // Written out: the list and each file at its path, the guide when asked.
    let out = dir.join("out");
    let docs = dir.join("docs");
    std::fs::create_dir_all(&docs).expect("docs");
    put(
        &docs,
        "README.md",
        b"# Guide\n\nSee [sources](sources.md).\n",
    );
    put(
        &docs,
        "sources.md",
        b"# Sources\n\nBack to [the guide](README.md#top).\n",
    );
    let w = write(source, &out, &docs, true).expect("written");
    let site = &w.site;
    let back = std::fs::read(site.join(LIST)).expect("the list");
    assert_eq!(Catalog::parse(&back).expect("reads").catalog, read.catalog);
    assert_eq!(std::fs::read(site.join(&h.file)).expect("its file"), houses);
    let index = std::fs::read_to_string(site.join("index.html")).expect("the front page");
    assert!(
        index.contains("<a href=\"sources.html\">sources</a>"),
        "{index}"
    );
    let other = std::fs::read_to_string(site.join("sources.html")).expect("its other page");
    assert!(other.contains("<a href=\"index.html#top\">the guide</a>"));
    assert!(said(&w).contains("Held back"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Without a config, or with one naming no file, every packed pack is published, and one that
/// names no id or version is refused with the rest of what is wrong.
#[test]
fn every_pack_is_published_when_none_is_named_and_one_with_no_id_is_refused() {
    let dir = folder("every");
    put(
        &dir,
        "a.dat",
        &packed_bytes(
            &manifest("id = \"com.example.a\"\nversion = \"1.0.0\"\n"),
            "1",
        ),
    );
    let source = plan(&dir, None).expect("the source");
    assert_eq!(source.site.len(), 1);
    assert!(source.held_back.is_empty());
    put(&dir, CONFIG, b"name = \"Mine\"\nguide = true\n");
    let source = plan(&dir, None).expect("the source");
    assert_eq!(source.site.len(), 1);
    assert!(source.guide);

    put(&dir, "b.dat", &packed_bytes(&manifest(""), "2"));
    put(&dir, "c.dat", b"not a packed pack at all");
    let refused = plan(&dir, None).expect_err("refused");
    assert!(
        refused.contains("b.dat") && refused.contains("names an id"),
        "{refused}"
    );
    assert!(refused.contains("c.dat"), "{refused}");
    let named_missing = plan(
        &dir,
        Some(config("[[pack]]\nfile = \"gone.dat\"\n").expect("reads")),
    )
    .expect_err("refused");
    assert!(named_missing.contains("`gone.dat`"), "{named_missing}");
    assert!(config("[[pack]]\nfile = \"a.dat\"\nversoin = \"1.0.0\"\n").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two packs of one id refuse the source; a pack larger than the pages host serves is listed
/// under the source's `files_url` and written for uploading, and with no `files_url` is refused.
#[test]
fn a_pack_too_large_for_the_pages_host_is_listed_where_it_is_uploaded() {
    let dir = folder("large");
    let pack = packed_bytes(
        &manifest("id = \"com.example.big\"\nversion = \"2.0.0\"\n"),
        "9",
    );
    let mut big = pack.clone();
    big.resize(usize::try_from(PAGES_FILE_LIMIT).expect("fits") + 1, 0);
    put(&dir, "big.dat", &big);
    let refused = plan(&dir, None).expect_err("no files_url");
    assert!(refused.contains("files_url"), "{refused}");
    let c = config("files_url = \"https://files.example.org/art\"\n").expect("reads");
    let source = plan(&dir, Some(c)).expect("the source");
    assert!(source.site.is_empty());
    assert_eq!(source.upload.len(), 1);
    let list = Catalog::parse(source.list.as_bytes())
        .expect("reads")
        .catalog;
    assert_eq!(
        list.packs[0].file,
        format!("https://files.example.org/art/{}", source.upload[0].path)
    );
    assert!(plan(
        &dir,
        Some(config("files_url = \"http://x/\"\n").expect("reads"))
    )
    .is_err());

    put(&dir, "twin.dat", &pack);
    let c = config("files_url = \"https://files.example.org/art/\"\n").expect("reads");
    let twice = plan(&dir, Some(c)).expect_err("one id twice");
    assert!(
        twice.contains("another pack has the id com.example.big"),
        "{twice}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_packs_folder_comes_from_the_flag_or_the_variable_and_the_address_is_the_launchers() {
    let none = |_: &str| None;
    let set = |k: &str| (k == PACKS_VAR).then(|| "/packs".to_owned());
    assert_eq!(
        packs_dir(Some(PathBuf::from("/flag")), &set),
        Ok(PathBuf::from("/flag"))
    );
    assert_eq!(packs_dir(None, &set), Ok(PathBuf::from("/packs")));
    assert!(packs_dir(None, &none).is_err_and(|e| e.contains(PACKS_VAR)));
    assert_eq!(URL, format!("https://art.dereth.network/{LIST}"));
    assert_eq!(day_of(std::time::UNIX_EPOCH), "1970-01-01");
    assert_eq!(
        day_of(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_794_096_000)),
        "2026-11-08"
    );
}

/// The tree's own guide renders, and every link in it to another page of the guide goes to that
/// page's HTML.
#[test]
fn the_trees_guide_renders_with_its_pages_linked() {
    let docs = crate::util::workspace_root().join(GUIDE);
    let pages = guide_pages(&docs).expect("the guide");
    let names: Vec<&str> = pages.iter().map(|(f, _)| f.as_str()).collect();
    assert!(
        names.contains(&"index.html") && names.contains(&"sources.html"),
        "{names:?}"
    );
    for (file, html) in &pages {
        for href in html.split("href=\"").skip(1) {
            let target = &href[..href.find('"').expect("closed")];
            assert!(
                !target
                    .split('#')
                    .next()
                    .unwrap_or_default()
                    .ends_with(".md")
                    || target.starts_with(crate::package::DEFAULT_SOURCE_URL),
                "{file}: {target} is left at a Markdown file of the guide"
            );
        }
    }
}

/// The guide's links are made the site's, whatever its wording: a page of the guide becomes its
/// page, anchor kept, and a page of the specification outside it the tree's copy.
#[test]
fn the_guides_links_to_its_pages_and_the_specification_are_made_the_sites() {
    let dir = folder("guide");
    put(
        &dir,
        "README.md",
        b"# Guide

See [sources](sources.md).

## Stacking
",
    );
    put(
        &dir,
        "sources.md",
        b"# Sources

[Stacking](README.md#stacking), [plugins](../plugins/sources.md).
",
    );
    let pages = guide_pages(&dir).expect("the guide");
    let page = |name: &str| &pages.iter().find(|(f, _)| f == name).expect(name).1;
    assert!(page("index.html").contains("href=\"sources.html\""));
    let other = page("sources.html");
    assert!(other.contains("href=\"index.html#stacking\""), "{other}");
    assert!(
        other.contains(&format!(
            "{}/blob/main/docs/plugins/sources.md",
            crate::package::DEFAULT_SOURCE_URL
        )),
        "{other}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A source whose front page is another site's sends the host's `/` there in a `_redirects`
/// beside the list; a `front` that is not https, or one given with the guide, is refused, and a
/// source with neither writes no redirects.
#[test]
fn a_front_elsewhere_sends_the_hosts_front_page_there() {
    let dir = folder("front");
    let c = config("front = \"https://dereth.network/art/\"\n").expect("reads");
    let source = plan(&dir, Some(c)).expect("the source");
    assert_eq!(source.front, "https://dereth.network/art/");
    let out = dir.join("out");
    let w = write(source, &out, Path::new("."), false).expect("written");
    assert_eq!(
        std::fs::read_to_string(w.site.join(REDIRECTS)).expect("redirects"),
        "/ https://dereth.network/art/ 302\n"
    );

    for bad in [
        "front = \"http://dereth.network/art/\"\n",
        "front = \"https://dereth.network/art/\"\nguide = true\n",
    ] {
        let refused = plan(&dir, Some(config(bad).expect("reads"))).expect_err(bad);
        assert!(refused.contains("front"), "{refused}");
    }

    let plain = plan(&dir, Some(Config::default())).expect("the source");
    let w = write(plain, &out, Path::new("."), false).expect("written");
    assert!(!w.site.join(REDIRECTS).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Dereth's source is configured by the tree's own file, which reads and sends the host's front
/// page to the guide; a config left in the packs folder is refused rather than read.
#[test]
fn dereths_source_is_configured_by_the_trees_file_alone() {
    let ws = crate::util::workspace_root();
    let c = tree_config(&ws).expect("the tree's config reads");
    let dir = folder("tree-config");
    let source = plan(&dir, Some(c)).expect("the source");
    assert!(source.front.starts_with("https://"), "{}", source.front);
    assert!(!source.guide);

    put(&dir, CONFIG, b"name = \"Elsewhere\"\n");
    let refused = build(&ws, &dir, &dir.join("out"), false).expect_err("a folder's own config");
    assert!(refused.contains(TREE_CONFIG), "{refused}");
    let _ = std::fs::remove_dir_all(&dir);
}
