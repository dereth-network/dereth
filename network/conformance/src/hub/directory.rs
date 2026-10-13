//! The directory's checks: what a hub's public routes answer while worlds list their characters
//! over their links, read as a page on another origin reads them ([`super::http`]).
//!
//! Each check lists characters of its own, named with a word no other check of the run uses, finds
//! them by that word, and takes them out again when it ends, passed or not, so that a run leaves
//! nothing listed. A hub that keeps no directory (it answers a listing `unavailable`) is not held
//! to them.

use std::time::Duration;

use dereth_network_protocol::directory::{Allegiance, Skill, Training};
use dereth_network_protocol::{Address, DirectoryMessage, ListingOutcome, Profile};
use serde_json::{json, Value};

use super::http::{encode, get, Answer};
use super::{addr, open_greeted, HubTarget, Link, WAIT};
use crate::Check;

/// The fields of a search's results; a character's own answer has these and its profile's.
const SUMMARY: &[&str] = &[
    "address",
    "name",
    "world",
    "updated",
    "level",
    "heritage",
    "gender",
    "title",
    "allegiance",
];

/// Every field a profile names.
const PROFILE: &[&str] = &[
    "level",
    "heritage",
    "gender",
    "title",
    "experience",
    "credits",
    "luminance",
    "attributes",
    "vitals",
    "skills",
    "augmentations",
    "allegiance",
    "birth",
    "age",
    "deaths",
    "chess",
    "fishing",
    "titles",
];

/// The six fields a character's "Allow others to see your ..." options govern.
const SEEABLE: &[&str] = &["birth", "age", "deaths", "chess", "fishing", "titles"];

const HOUR: Duration = Duration::from_secs(3_600);

/// One check's links, by world, and the characters it listed, taken out again when it ends.
struct Lister<'a> {
    t: &'a HubTarget,
    links: Vec<Link>,
    listed: Vec<(usize, Address)>,
    tag: String,
    sent: usize,
}

impl Lister<'_> {
    /// `first` and the check's word, a character of world `w`.
    fn name(&self, w: usize, first: &str, word: &str) -> Address {
        addr(&format!("{first} {word}"), &self.t.worlds[w].0)
    }

    /// Sends `frame`, a listing, on world `w`'s link: the hub's answer.
    fn send(&mut self, w: usize, frame: &str) -> Result<ListingOutcome, String> {
        let id = serde_json::from_str::<Value>(frame)
            .ok()
            .and_then(|v| v["id"].as_str().map(str::to_owned))
            .ok_or("a listing with no id")?;
        let link = &mut self.links[w];
        link.send_raw(frame)?;
        link.expect_directory(WAIT, "the listing's answer", |m| match m {
            DirectoryMessage::ListingResult { id: i, outcome, .. } if *i == id => Some(*outcome),
            _ => None,
        })
    }

    fn next_id(&mut self) -> String {
        self.sent += 1;
        format!("dir{}{}", self.sent, self.tag)
    }

    /// `who` listed with `profile`, its frame as `frame` leaves it.
    fn list_as(
        &mut self,
        w: usize,
        who: &Address,
        profile: &Profile,
        frame: impl FnOnce(&mut Value),
    ) -> Result<(), String> {
        let mut v: Value = serde_json::from_str(
            &DirectoryMessage::Listing {
                id: self.next_id(),
                who: who.clone(),
                on: true,
                profile: Some(Box::new(profile.clone())),
            }
            .to_json(),
        )
        .map_err(|e| e.to_string())?;
        frame(&mut v);
        let outcome = self.send(w, &v.to_string())?;
        if !self.listed.iter().any(|(_, a)| a.same(who)) {
            self.listed.push((w, who.clone()));
        }
        match outcome {
            ListingOutcome::Ok => Ok(()),
            other => Err(format!("listing {who}: {other:?}")),
        }
    }

    fn list(&mut self, w: usize, who: &Address, profile: &Profile) -> Result<(), String> {
        self.list_as(w, who, profile, |_| {})
    }

    /// `who` taken out by its world.
    fn unlist(&mut self, w: usize, who: &Address) -> Result<(), String> {
        let frame = DirectoryMessage::Listing {
            id: self.next_id(),
            who: who.clone(),
            on: false,
            profile: None,
        }
        .to_json();
        let outcome = self.send(w, &frame)?;
        self.listed.retain(|(_, a)| !a.same(who));
        match outcome {
            ListingOutcome::Ok => Ok(()),
            other => Err(format!("taking {who} out: {other:?}")),
        }
    }

    /// Lets the hub's time pass, when the suite holds its clock.
    fn pass(&self, by: Duration) {
        if let Some(clock) = &self.t.clock {
            clock.advance(by);
        }
    }
}

/// Runs `f` with links of the first `worlds` worlds, then takes out whatever it left listed.
fn with_links(
    t: &HubTarget,
    tg: &str,
    worlds: usize,
    f: impl FnOnce(&mut Lister) -> Result<(), String>,
) -> Result<(), String> {
    let mut links = Vec::new();
    for i in 0..worlds {
        links.push(open_greeted(t, i)?);
    }
    let mut l = Lister {
        t,
        links,
        listed: Vec::new(),
        tag: tg.to_owned(),
        sent: 0,
    };
    let result = f(&mut l);
    let mut cleaned = Ok(());
    for (w, who) in l.listed.clone() {
        let out = l.unlist(w, &who);
        if cleaned.is_ok() {
            cleaned = out;
        }
    }
    for link in l.links {
        link.close();
    }
    result.and(cleaned)
}

/// A check's word: `X`, a letter of its own, and the run's tag.
fn word(c: &str, tg: &str) -> String {
    format!("X{c}{tg}")
}

/// A smallest profile, with a title.
fn titled(level: u32) -> Profile {
    Profile {
        level,
        heritage: 1,
        gender: 1,
        title: Some("the Bloodletter".into()),
        ..Profile::default()
    }
}

/// A profile in the allegiance of `word`, at `rank`, naming `patron` and `monarch`.
fn sworn(
    level: u32,
    word: &str,
    rank: u32,
    patron: Option<&Address>,
    monarch: Option<&Address>,
) -> Profile {
    Profile {
        allegiance: Some(Allegiance {
            key: allegiance_key(word),
            name: Some(format!("Order {word}")),
            members: 41,
            rank,
            patron: patron.cloned(),
            monarch: monarch.cloned(),
        }),
        ..titled(level)
    }
}

fn allegiance_key(word: &str) -> String {
    format!("conformance-{word}")
}

fn character_path(address: &str) -> String {
    format!("/v1/directory/characters/{}", encode(address))
}

fn character(t: &HubTarget, who: &Address) -> Result<Answer, String> {
    get(&t.base, &character_path(&who.to_string()))
}

/// `a`'s body, which must answer `200`.
fn ok_json(a: &Answer, what: &str) -> Result<Value, String> {
    if a.status != 200 {
        return Err(format!("{what}: {} {}", a.status, a.body));
    }
    a.json()
}

/// A listed character's own answer.
fn published(t: &HubTarget, who: &Address) -> Result<Value, String> {
    Ok(ok_json(&character(t, who)?, &who.to_string())?["character"].clone())
}

/// A search's page, which must answer `200`.
fn search(t: &HubTarget, query: &str) -> Result<Value, String> {
    let a = get(&t.base, &format!("/v1/directory/characters?{query}"))?;
    ok_json(&a, &format!("the search {query}"))
}

/// The addresses a page names, in its order.
fn addresses(page: &Value) -> Vec<String> {
    page["data"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r["address"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn named(who: &[&Address]) -> Vec<String> {
    who.iter().map(ToString::to_string).collect()
}

/// The query that finds a check's characters by its word.
fn by_word(word: &str) -> String {
    format!("q={}", encode(&word.to_lowercase()))
}

/// Whether `s` is an hour, as the directory keeps the time a profile changed: no minute, second
/// or fraction.
fn is_hour(s: &str) -> bool {
    let b = s.as_bytes();
    s.len() == 24 && b[4] == b'-' && b[10] == b'T' && s.ends_with(":00:00.000Z")
}

/// A page's paging fields: `total`, `limit`, `offset`, `nextOffset`.
fn paging(page: &Value) -> (Value, Value, Value, Value) {
    (
        page["total"].clone(),
        page["limit"].clone(),
        page["offset"].clone(),
        page["nextOffset"].clone(),
    )
}

/// What a page of another origin may do with a directory answer: read it, and keep it a minute.
fn public_for_a_minute(a: &Answer, what: &str) -> Result<(), String> {
    match (
        a.header("cache-control"),
        a.header("access-control-allow-origin"),
    ) {
        (Some("public, max-age=60"), Some("*")) => Ok(()),
        other => Err(format!("{what}: cached and shared as {other:?}")),
    }
}

/// Whether the hub keeps a directory: its answer to a listing of the world's own character.
fn keeps_a_directory(t: &HubTarget, tg: &str) -> Result<bool, String> {
    let mut keeps = false;
    with_links(t, tg, 1, |d| {
        let probe = d.name(0, "Probe", &word("z", tg));
        let id = d.next_id();
        let frame = DirectoryMessage::Listing {
            id,
            who: probe.clone(),
            on: true,
            profile: Some(Box::new(titled(1))),
        }
        .to_json();
        match d.send(0, &frame)? {
            ListingOutcome::Ok => {
                keeps = true;
                d.listed.push((0, probe));
                Ok(())
            }
            ListingOutcome::Unavailable => Ok(()),
            other => Err(format!("a listing of the world's own character: {other:?}")),
        }
    })?;
    Ok(keeps)
}

/// Runs the directory's checks against `t`, `tg` being the run's tag.
pub(super) fn checks(t: &HubTarget, tg: &str) -> Vec<Check> {
    let mut out = Vec::new();
    match keeps_a_directory(t, tg) {
        Ok(true) => {}
        Ok(false) => return out,
        Err(e) => {
            out.push(Check::new("hub", "directory.setup", Err(e)));
            return out;
        }
    }
    let mut check = |name: &'static str, f: &dyn Fn() -> Result<(), String>| {
        out.push(Check::new("hub", name, f()));
    };
    check(
        "directory.a_listing_is_public_at_once_and_gone_at_once",
        &|| public_at_once(t, tg),
    );
    check("directory.absence_is_one_404", &|| {
        absence_is_one_404(t, tg)
    });
    check("directory.a_patron_shows_only_while_listed", &|| {
        a_patron_shows_only_while_listed(t, tg)
    });
    check(
        "directory.an_allegiances_listed_members_come_highest_rank_first",
        &|| listed_members(t, tg),
    );
    check(
        "directory.only_the_profiles_own_fields_are_published",
        &|| own_fields(t, tg),
    );
    check(
        "directory.a_search_is_paged_within_its_first_thousand_results",
        &|| paged(t, tg),
    );
    check("directory.a_search_field_out_of_its_bounds_is_400", &|| {
        bounds(t, tg)
    });
    // The two that need the hub's hours to pass come last: its clock only goes forward.
    check("directory.an_unchanged_profile_keeps_its_hour", &|| {
        keeps_its_hour(t, tg)
    });
    check(
        "directory.a_search_sorts_by_the_latest_change_or_by_level_then_by_name",
        &|| sorted(t, tg),
    );
    out
}

fn public_at_once(t: &HubTarget, tg: &str) -> Result<(), String> {
    let w = word("p", tg);
    with_links(t, tg, 1, |d| {
        let who = d.name(0, "Ann", &w);
        d.list(0, &who, &titled(126))?;
        let a = character(t, &who)?;
        let c = ok_json(&a, "the character, at once")?["character"].clone();
        public_for_a_minute(&a, "the character")?;
        let want = (
            json!(who.to_string()),
            json!(format!("Ann {w}")),
            json!(t.worlds[0].0),
            json!([126, 1, 1]),
            json!("the Bloodletter"),
        );
        let got = (
            c["address"].clone(),
            c["name"].clone(),
            c["world"]["slug"].clone(),
            json!([c["level"], c["heritage"], c["gender"]]),
            c["title"].clone(),
        );
        if got != want {
            return Err(format!("published as {c}"));
        }
        if !c["updated"].as_str().is_some_and(is_hour) {
            return Err(format!(
                "changed at {}: an hour, and no finer",
                c["updated"]
            ));
        }
        let lower = get(&t.base, &character_path(&who.to_string().to_lowercase()))?;
        if ok_json(&lower, "the address in lower case")?["character"]["address"] != want.0 {
            return Err(format!("the address in lower case: {}", lower.body));
        }
        let found = search(t, &by_word(&w))?;
        if addresses(&found) != [who.to_string()] || found["total"] != 1 {
            return Err(format!("the search: {found}"));
        }
        d.unlist(0, &who)?;
        let gone = character(t, &who)?;
        if gone.status != 404 {
            return Err(format!("taken out, its page answers {}", gone.status));
        }
        let found = search(t, &by_word(&w))?;
        if !addresses(&found).is_empty() || found["total"] != 0 {
            return Err(format!("taken out, the search: {found}"));
        }
        Ok(())
    })
}

/// What an answer is to whoever reads it: its status, its body, and what a cache may do with it.
type Seen = (u16, String, Option<String>);

fn absence_is_one_404(t: &HubTarget, tg: &str) -> Result<(), String> {
    let w = word("a", tg);
    with_links(t, tg, 1, |d| {
        let slug = t.worlds[0].0.clone();
        let gone = d.name(0, "Gone", &w);
        d.list(0, &gone, &sworn(20, &w, 1, None, None))?;
        d.unlist(0, &gone)?;
        let never = format!("Never {w}");
        let mut absent = vec![
            ("taken out", character_path(&gone.to_string())),
            ("never listed", character_path(&format!("{never}@{slug}"))),
            (
                "of a world not on the network",
                character_path(&format!("{never}@nowhere{}", &tg[..tg.len().min(4)])),
            ),
            ("a channel", character_path("oldguard@channel")),
            ("not an address", character_path("not an address")),
            (
                "no character's name",
                character_path(&format!("Never<{w}@{slug}")),
            ),
        ];
        if let Some((off, _)) = &t.off {
            absent.push((
                "of a world whose flag is not on",
                character_path(&format!("{never}@{off}")),
            ));
        }
        let none_listed = format!(
            "/v1/directory/allegiances/{slug}/{}",
            encode(&allegiance_key(&w))
        );
        let allegiances = [
            ("an allegiance whose last member was taken out", none_listed),
            (
                "an allegiance key that names none",
                format!(
                    "/v1/directory/allegiances/{slug}/{}",
                    allegiance_key(&format!("{w}none"))
                ),
            ),
            (
                "no allegiance key",
                format!("/v1/directory/allegiances/{slug}/x"),
            ),
        ];
        for group in [&absent[..], &allegiances[..]] {
            let mut first: Option<(&str, Seen)> = None;
            for (what, path) in group {
                let a = get(&t.base, path)?;
                let seen = (
                    a.status,
                    a.body.clone(),
                    a.header("cache-control").map(str::to_owned),
                );
                match &first {
                    None => {
                        if seen.0 != 404 || seen.2.as_deref() != Some("no-store") {
                            return Err(format!("{what}: {seen:?}, not a 404 kept by no cache"));
                        }
                        first = Some((what, seen));
                    }
                    Some((like, f)) if *f != seen => {
                        return Err(format!("{what} answers {seen:?}; {like}, {f:?}"));
                    }
                    Some(_) => {}
                }
            }
        }
        Ok(())
    })
}

fn a_patron_shows_only_while_listed(t: &HubTarget, tg: &str) -> Result<(), String> {
    let w = word("n", tg);
    with_links(t, tg, 1, |d| {
        let (vas, pat, mon) = (
            d.name(0, "Vas", &w),
            d.name(0, "Pat", &w),
            d.name(0, "Mon", &w),
        );
        let vassal = sworn(10, &w, 1, Some(&pat), Some(&mon));
        let patron = sworn(20, &w, 2, Some(&mon), Some(&mon));
        let names = |who: &Address| -> Result<(Value, Value), String> {
            let a = published(t, who)?;
            let al = &a["allegiance"];
            Ok((al["patron"].clone(), al["monarch"].clone()))
        };
        let expect = |who: &Address, patron: Option<&Address>, monarch: Option<&Address>, when| {
            let want = (
                patron.map_or(Value::Null, |a| json!(a.to_string())),
                monarch.map_or(Value::Null, |a| json!(a.to_string())),
            );
            let got = names(who)?;
            if got == want {
                Ok(())
            } else {
                Err(format!("{when}, {who} names {got:?}, not {want:?}"))
            }
        };
        d.list(0, &vas, &vassal)?;
        expect(&vas, None, None, "neither listed")?;
        let body = character(t, &vas)?.body.to_lowercase();
        for unlisted in [&pat, &mon] {
            if body.contains(&unlisted.name().to_lowercase()) {
                return Err(format!("{unlisted}, not listed, is in its vassal's answer"));
            }
        }
        d.list(0, &pat, &patron)?;
        expect(&vas, Some(&pat), None, "its patron listed")?;
        d.list(0, &mon, &sworn(30, &w, 3, None, None))?;
        expect(&vas, Some(&pat), Some(&mon), "both listed")?;
        expect(&pat, Some(&mon), Some(&mon), "both listed")?;
        d.unlist(0, &pat)?;
        expect(&vas, None, Some(&mon), "its patron taken out")?;
        // Struck from the listings that named it, not hidden: listed again, it is named once its
        // vassal's world names it again.
        d.list(0, &pat, &patron)?;
        expect(&vas, None, Some(&mon), "its patron listed again")?;
        d.list(0, &vas, &vassal)?;
        expect(&vas, Some(&pat), Some(&mon), "its vassal listed again")?;
        d.unlist(0, &mon)?;
        expect(&vas, Some(&pat), None, "its monarch taken out")?;
        expect(&pat, None, None, "its monarch taken out")
    })
}

fn listed_members(t: &HubTarget, tg: &str) -> Result<(), String> {
    let w = word("m", tg);
    with_links(t, tg, 2, |d| {
        let slug = t.worlds[1].0.clone();
        let [mon, pat, amy, bea, cy] = ["Mon", "Pat", "Amy", "Bea", "Cy"].map(|n| d.name(1, n, &w));
        let zed = d.name(1, "Zed", &w);
        d.list(1, &cy, &sworn(10, &w, 1, Some(&zed), Some(&mon)))?;
        d.list(1, &bea, &sworn(10, &w, 1, Some(&pat), Some(&mon)))?;
        d.list(1, &amy, &sworn(10, &w, 1, Some(&pat), Some(&mon)))?;
        d.list(1, &pat, &sworn(20, &w, 3, Some(&mon), Some(&mon)))?;
        d.list(1, &mon, &sworn(30, &w, 5, None, None))?;
        let path = format!(
            "/v1/directory/allegiances/{slug}/{}",
            encode(&allegiance_key(&w))
        );
        let a = get(&t.base, &path)?;
        let all = ok_json(&a, "the allegiance")?;
        public_for_a_minute(&a, "the allegiance")?;
        let al = &all["allegiance"];
        let got = [
            &al["world"]["slug"],
            &al["key"],
            &al["name"],
            &al["members"],
            &al["listed"],
        ];
        let want = [
            &json!(slug),
            &json!(allegiance_key(&w)),
            &json!(format!("Order {w}")),
            &json!(41),
            &json!(5),
        ];
        if got != want {
            return Err(format!("the allegiance: {al}"));
        }
        if addresses(&all) != named(&[&mon, &pat, &amy, &bea, &cy]) || all["total"] != 5 {
            return Err(format!("highest rank first, then by name: {all}"));
        }
        let rows = all["data"].as_array().cloned().unwrap_or_default();
        let patron_of = |i: usize| rows.get(i).map(|r| r["allegiance"]["patron"].clone());
        if patron_of(2) != Some(json!(pat.to_string())) || patron_of(4) != Some(Value::Null) {
            return Err(format!("a member's patron, while listed: {all}"));
        }
        if a.body.to_lowercase().contains(&zed.name().to_lowercase()) {
            return Err(format!("{zed}, not listed, is named"));
        }
        let first = ok_json(&get(&t.base, &format!("{path}?limit=2"))?, "its first page")?;
        let last = ok_json(
            &get(&t.base, &format!("{path}?limit=2&offset=4"))?,
            "its last",
        )?;
        let pages = (
            addresses(&first),
            paging(&first),
            addresses(&last),
            paging(&last),
        );
        let want = (
            named(&[&mon, &pat]),
            (json!(5), json!(2), json!(0), json!(2)),
            named(&[&cy]),
            (json!(5), json!(2), json!(4), Value::Null),
        );
        if pages != want {
            return Err(format!("paged: {pages:?}"));
        }
        Ok(())
    })
}

fn own_fields(t: &HubTarget, tg: &str) -> Result<(), String> {
    let w = word("f", tg);
    with_links(t, tg, 1, |d| {
        let who = d.name(0, "Pri", &w);
        let profile = Profile {
            deaths: Some(12),
            skills: vec![Skill {
                id: 6,
                training: Training::Specialized,
                base: 420,
            }],
            ..titled(40)
        };
        // A later version's fields, and fields no version names, as a careless world might send.
        d.list_as(0, &who, &profile, |frame| {
            let p = &mut frame["profile"];
            p["online"] = json!(true);
            p["lastSeen"] = json!("2026-10-12T14:03:11.000Z");
            p["account"] = json!("acct-conformance");
            p["house"] = json!("Villa Conformance");
            p["skills"][0]["note"] = json!("x");
            frame["person"] = json!("Nelly Conformance");
        })?;
        let a = character(t, &who)?;
        let c = ok_json(&a, "the character")?["character"].clone();
        let keys: Vec<String> = c
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        if let Some(k) = keys
            .iter()
            .find(|k| !SUMMARY.contains(&k.as_str()) && !PROFILE.contains(&k.as_str()))
        {
            return Err(format!("published {k}, which no profile names: {c}"));
        }
        if let Some(k) = SEEABLE
            .iter()
            .find(|k| **k != "deaths" && c.get(**k).is_some())
        {
            return Err(format!("published {k}, which the world left out: {c}"));
        }
        if c["deaths"] != 12
            || c["skills"] != json!([{"id": 6, "training": "specialized", "base": 420}])
        {
            return Err(format!("not as the world sent it: {c}"));
        }
        for sent in ["acct-conformance", "villa conformance", "nelly conformance"] {
            if a.body.to_lowercase().contains(sent) {
                return Err(format!("published {sent:?}, which no profile names"));
            }
        }
        let found = search(t, &by_word(&w))?;
        for row in found["data"].as_array().cloned().unwrap_or_default() {
            if let Some(k) = row
                .as_object()
                .and_then(|o| o.keys().find(|k| !SUMMARY.contains(&k.as_str())).cloned())
            {
                return Err(format!("a search's result carries {k}: {row}"));
            }
        }
        if addresses(&found) != [who.to_string()] {
            return Err(format!("the search: {found}"));
        }
        Ok(())
    })
}

fn paged(t: &HubTarget, tg: &str) -> Result<(), String> {
    let w = word("g", tg);
    with_links(t, tg, 1, |d| {
        let [amy, ben, cal] = ["Amy", "Ben", "Cal"].map(|n| d.name(0, n, &w));
        d.list(0, &cal, &titled(10))?;
        d.list(0, &amy, &titled(30))?;
        d.list(0, &ben, &titled(20))?;
        let q = format!("{}&sort=level", by_word(&w));
        let first = search(t, &format!("{q}&limit=2"))?;
        let second = search(t, &format!("{q}&limit=2&offset=2"))?;
        let reach = search(t, &format!("{q}&offset=950"))?;
        let got = (
            (addresses(&first), paging(&first)),
            (addresses(&second), paging(&second)),
            (addresses(&reach), paging(&reach)),
        );
        let want = (
            (
                named(&[&amy, &ben]),
                (json!(3), json!(2), json!(0), json!(2)),
            ),
            (named(&[&cal]), (json!(3), json!(2), json!(2), Value::Null)),
            (Vec::new(), (json!(3), json!(50), json!(950), Value::Null)),
        );
        if got == want {
            Ok(())
        } else {
            Err(format!("paged: {got:?}"))
        }
    })
}

fn bounds(t: &HubTarget, tg: &str) -> Result<(), String> {
    let q = by_word(&word("b", tg));
    let refused: [(&str, &str); 11] = [
        ("q", "q=b"),
        ("q", "q=bo%3Cb"),
        ("world", "world=No%20World"),
        ("level_min", "level_min=0"),
        ("level_max", "level_max=100001"),
        ("level_min", "level_min=9&level_max=8"),
        ("heritage", "heritage=14"),
        ("sort", "sort=name"),
        ("limit", "limit=0"),
        ("limit", "limit=51"),
        ("", "offset=951"),
    ];
    for (field, query) in refused {
        let a = get(&t.base, &format!("/v1/directory/characters?{query}"))?;
        let message = a.json().ok().and_then(|v| {
            v["error"]["message"]
                .as_str()
                .map(str::to_owned)
                .filter(|_| v["error"]["code"].is_string())
        });
        match message {
            Some(m) if a.status == 400 && m.contains(field) => {}
            _ => return Err(format!("{query}: {} {}", a.status, a.body)),
        }
    }
    search(
        t,
        &format!("{q}&level_min=1&level_max=100000&heritage=13&limit=1&offset=999"),
    )
    .map(|_| ())
    .map_err(|e| format!("every field at its bound: {e}"))
}

fn keeps_its_hour(t: &HubTarget, tg: &str) -> Result<(), String> {
    let w = word("h", tg);
    with_links(t, tg, 1, |d| {
        let who = d.name(0, "Hal", &w);
        d.list(0, &who, &titled(10))?;
        let first = published(t, &who)?;
        let then = first["updated"].as_str().unwrap_or("").to_owned();
        if !is_hour(&then) {
            return Err(format!("changed at {then:?}: an hour, and no finer"));
        }
        d.pass(2 * HOUR);
        d.list(0, &who, &titled(10))?;
        let again = published(t, &who)?;
        if again != first {
            return Err(format!("sent again unchanged, {first} became {again}"));
        }
        d.list(0, &who, &titled(11))?;
        let changed = published(t, &who)?;
        let now = changed["updated"].as_str().unwrap_or("").to_owned();
        let later = if t.clock.is_some() {
            now > then
        } else {
            now >= then
        };
        if changed["level"] != 11 || !is_hour(&now) || !later {
            return Err(format!("changed, {changed}, after {then}"));
        }
        Ok(())
    })
}

fn sorted(t: &HubTarget, tg: &str) -> Result<(), String> {
    let w = word("s", tg);
    with_links(t, tg, 2, |d| {
        let (ann, bob, cid, dan) = (
            d.name(0, "Ann", &w),
            d.name(1, "Bob", &w),
            d.name(0, "Cid", &w),
            d.name(1, "Dan", &w),
        );
        d.list(0, &ann, &titled(126))?;
        d.pass(HOUR);
        d.list(1, &bob, &titled(50))?;
        d.pass(HOUR);
        d.list(0, &cid, &titled(126))?;
        d.list(1, &dan, &titled(10))?;
        let latest = search(t, &by_word(&w))?;
        // Most lately changed first, then by name, as the hub says each changed: hours apart when
        // the suite holds the hub's clock, otherwise most likely the same hour.
        let mut rows = latest["data"].as_array().cloned().unwrap_or_default();
        rows.sort_by(|a, b| {
            let name = |r: &Value| r["name"].as_str().unwrap_or("").to_lowercase();
            let hour = |r: &Value| r["updated"].as_str().unwrap_or("").to_owned();
            hour(b).cmp(&hour(a)).then_with(|| name(a).cmp(&name(b)))
        });
        let want: Vec<String> = rows
            .iter()
            .filter_map(|r| r["address"].as_str().map(str::to_owned))
            .collect();
        let got = addresses(&latest);
        if got != want || latest["total"] != 4 || got.len() != 4 {
            return Err(format!("by the latest change: {got:?}, not {want:?}"));
        }
        if t.clock.is_some() {
            let hour = |who: &Address| -> String {
                rows.iter()
                    .find(|r| r["address"] == who.to_string())
                    .and_then(|r| r["updated"].as_str())
                    .unwrap_or("")
                    .to_owned()
            };
            let (a, b, c, d) = (hour(&ann), hour(&bob), hour(&cid), hour(&dan));
            if !(a < b && b < c && b < d) {
                return Err(format!("changed hours apart, at {a}, {b}, {c} and {d}"));
            }
        }
        let by_level = addresses(&search(t, &format!("{}&sort=level", by_word(&w)))?);
        if by_level != named(&[&ann, &cid, &bob, &dan]) {
            return Err(format!("by level: {by_level:?}"));
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::is_hour;

    /// Spec: conformance: the hour a profile last changed is read as an hour, never a finer time.
    #[test]
    fn only_a_whole_hour_reads_as_an_hour() {
        assert!(is_hour("2026-10-12T14:00:00.000Z"));
        for finer in [
            "2026-10-12T14:03:00.000Z",
            "2026-10-12T14:00:07.000Z",
            "2026-10-12T14:00:00.250Z",
            "2026-10-12 14:00:00.000Z",
            "14:00",
        ] {
            assert!(!is_hour(finer), "{finer}");
        }
    }
}
