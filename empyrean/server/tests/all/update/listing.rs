//! Divergence: V385
//! ACE: Program.CheckForServerUpdate (ACE asks only for the newest ACE release; Empyrean reads its
//! repository's release list, which every product of the repository shares)
//! Behaviour: none (a server tooling claim: how much of the release list is read, not game
//! behaviour).
//! The release list is read a page at a time through each answer's `Link` header, down to the
//! first page holding a release at or below the running version and no further, at most ten
//! pages; a newer release that other products' releases pushed off the first page is still found;
//! a page refused partway fails the check; a next page on another host is not read.
//! Fixture: synthetic release lists, served on 127.0.0.1 and from memory.

use empyrean_server::update::source::{self, Published, Source, MAX_PAGES};
use empyrean_server::update::version::Version;

use super::Loopback;

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

/// A page of the list holding releases tagged `tags`.
fn page(tags: &[&str]) -> String {
    let releases: Vec<String> = tags
        .iter()
        .map(|t| format!(r#"{{"tag_name": "{t}", "prerelease": false, "assets": []}}"#))
        .collect();
    format!("[{}]", releases.join(","))
}

fn versions(found: &[Published]) -> Vec<String> {
    found.iter().map(|p| p.version.to_string()).collect()
}

/// The releases `pages` list from memory, each page naming the next on `next_host` as GitHub
/// does, read for a server at `current`; and the pages asked for.
fn from_memory(
    pages: &[Result<Vec<&str>, String>],
    current: &str,
    next_host: &str,
) -> (Result<Vec<Published>, String>, Vec<usize>) {
    let source = Source::github("https://github.com/example/empyrean").unwrap();
    let mut asked = Vec::new();
    let found = source::list_pages(&source, &v(current), &mut |url| {
        let n = url
            .rsplit_once("&page=")
            .map_or(1, |(_, n)| n.parse::<usize>().unwrap());
        asked.push(n);
        let tags = pages[n - 1].clone()?;
        let at = format!("{next_host}/repositories/1/releases?per_page=100");
        let next = (n < pages.len())
            .then(|| {
                source::next_page(&format!(
                    "<{at}&page={}>; rel=\"next\", <{at}&page={}>; rel=\"last\"",
                    n + 1,
                    pages.len()
                ))
            })
            .flatten();
        Ok((page(&tags), next))
    });
    (found, asked)
}

#[test]
fn the_list_is_read_through_its_link_headers_down_to_the_running_version() {
    let web = Loopback::start();
    let api = format!("{}/repos/example/empyrean", web.base);
    let first = "/repos/example/empyrean/releases?per_page=100";
    let nth = |n: usize| format!("{first}&page={n}");
    let link = |n: usize| {
        format!(
            "<{}{}>; rel=\"next\", <{}{}>; rel=\"last\"",
            web.base,
            nth(n),
            web.base,
            nth(4)
        )
    };
    web.put(first, page(&["empyrean-v0.4.0", "network-hub-v0.2.4"]));
    web.links.lock().unwrap().insert(first.to_owned(), link(2));
    web.put(&nth(2), page(&["lyceum-v0.1.0", "empyrean-v0.3.5"]));
    web.links.lock().unwrap().insert(nth(2), link(3));
    web.put(&nth(3), page(&["empyrean-v0.3.4", "empyrean-v0.3.3"]));
    web.links.lock().unwrap().insert(nth(3), link(4));
    web.put(&nth(4), page(&["empyrean-v0.3.2"]));

    let source = Source::configured(&api).unwrap();
    let found = source::list(&source::agent(), &source, &v("0.3.4")).unwrap();
    assert_eq!(versions(&found), ["0.4.0", "0.3.5", "0.3.4", "0.3.3"]);
    assert_eq!(
        *web.asked.lock().unwrap(),
        [first.to_owned(), nth(2), nth(3)],
        "the fourth page holds nothing newer, and is not asked for"
    );

    // The newest on the first page: one request, as before.
    web.asked.lock().unwrap().clear();
    let found = source::list(&source::agent(), &source, &v("0.4.0")).unwrap();
    assert_eq!(versions(&found), ["0.4.0"]);
    assert_eq!(web.asked.lock().unwrap().len(), 1);
}

#[test]
fn a_newer_release_pushed_off_the_first_page_is_found_and_ten_pages_are_the_most_read() {
    let mut pages: Vec<Result<Vec<&str>, String>> = vec![Ok(vec!["dereth-v0.4.6"]); 4];
    pages.push(Ok(vec!["empyrean-v0.3.5", "empyrean-v0.3.4"]));
    let (found, asked) = from_memory(&pages, "0.3.4", "https://api.github.com");
    assert_eq!(versions(&found.unwrap()), ["0.3.5", "0.3.4"]);
    assert_eq!(asked, [1, 2, 3, 4, 5]);

    // A build whose own version was never released stops below it all the same.
    let pages = vec![
        Ok(vec!["empyrean-v0.3.5"]),
        Ok(vec!["empyrean-v0.3.5-rc.1"]),
        Ok(vec!["empyrean-v0.3.4"]),
    ];
    let (_, asked) = from_memory(&pages, "0.3.5-rc.2", "https://api.github.com");
    assert_eq!(asked, [1, 2]);

    let pages = vec![Ok(vec!["dereth-v0.4.6"]); MAX_PAGES + 3];
    let (found, asked) = from_memory(&pages, "0.3.4", "https://api.github.com");
    assert!(found.unwrap().is_empty());
    assert_eq!(asked, (1..=MAX_PAGES).collect::<Vec<_>>());
}

#[test]
fn a_page_refused_partway_fails_the_check_and_another_host_is_not_read() {
    let pages = vec![
        Ok(vec!["dereth-v0.4.6"]),
        Err("https://api.github.com/... refused the request (HTTP 403)".to_owned()),
        Ok(vec!["empyrean-v0.3.4"]),
    ];
    let (found, asked) = from_memory(&pages, "0.3.4", "https://api.github.com");
    assert!(found.unwrap_err().contains("HTTP 403"));
    assert_eq!(asked, [1, 2]);

    let pages = vec![Ok(vec!["dereth-v0.4.6"]), Ok(vec!["empyrean-v0.3.5"])];
    let (found, asked) = from_memory(&pages, "0.3.4", "https://elsewhere.test");
    assert!(found.unwrap().is_empty());
    assert_eq!(asked, [1]);
}
