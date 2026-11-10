//! The Library's downloads in a browser: a data set from the community's archive, got into the
//! browser's own storage. The steps are `dereth-datfetch`'s; this is what the page's worker sees of
//! them, as JSON, and the last check, that the files report the iterations the launcher matches
//! the set by ([`dereth_launch::downloads::check`]).
//!
//! A set's files are written to its folder (`sets/<id>`, or `auberean/sets/<id>` for Asheron's
//! Call 2's) while it is got, with a marker file in it ([`INCOMPLETE`]) until every file is in and
//! checked, so a folder half written is never offered as a set. Its archives are kept beside, in
//! `downloads/`, until each is unpacked: a download stopped part way goes on the next time.

use dereth_datfetch::{Catalogue, DownloadEnd, Fetch, Next, Step};
use dereth_launch::datset::{self, SetKind};
use dereth_launch::downloads;
use dereth_launch::world::Game;
use serde_json::json;

pub use dereth_launch::downloads::INCOMPLETE;

/// The list in use: the built-in one.
fn catalogue() -> Catalogue {
    Catalogue::built_in()
}

/// The folder a set of `game`'s is kept in.
#[must_use]
pub fn folder(game: Game, id: &str) -> String {
    match game {
        Game::Dereth => format!("sets/{id}"),
        Game::Auberean => format!("auberean/sets/{id}"),
    }
}

/// The sets on offer, as the page lists them ([`downloads::Offer`]), each with the folder it is
/// kept in.
#[must_use]
pub fn offers() -> String {
    let c = catalogue();
    let sets: Vec<_> = downloads::offers(&c)
        .into_iter()
        .map(|o| {
            let folder = folder(o.game, &o.id);
            let mut v = serde_json::to_value(o).unwrap_or_default();
            v["folder"] = json!(folder);
            v
        })
        .collect();
    json!({ "guide": c.guide, "sets": sets }).to_string()
}

/// One set being got.
#[derive(Debug)]
pub struct GetDats {
    fetch: Fetch,
    kind: SetKind,
    folder: String,
}

impl GetDats {
    /// Begin getting set `id`, with `kept[i]` bytes of its archive `i` kept from before.
    ///
    /// # Errors
    /// No such set is on offer.
    pub fn begin(id: &str, kept: &[u64]) -> Result<GetDats, String> {
        let c = catalogue();
        let set = c
            .set(id)
            .ok_or_else(|| format!("no data set {id:?} is on offer"))?;
        let (game, kind) = downloads::kind_of(set);
        Ok(GetDats {
            fetch: Fetch::new(set.clone(), kept),
            kind,
            folder: folder(game, id),
        })
    }

    /// The next step, as JSON (`kind` and its fields).
    pub fn step(&mut self) -> String {
        match self.fetch.step() {
            Step::Api {
                url,
                body,
                after_ms,
            } => {
                json!({ "kind": "api", "url": url, "body": body, "after_ms": after_ms })
            }
            Step::Download {
                archive,
                from,
                to,
                connections,
            } => json!({
                "kind": "download",
                "archive": archive,
                "from": from,
                "to": to,
                "connections": connections,
            }),
            Step::Read {
                archive,
                offset,
                len,
            } => json!({ "kind": "read", "archive": archive, "offset": offset, "len": len }),
            Step::Prepare { files } => json!({ "kind": "prepare", "files": files }),
            Step::Remove { archive } => json!({ "kind": "remove", "archive": archive }),
            Step::Done { files, note } => {
                json!({ "kind": "done", "files": files, "folder": self.folder, "note": note })
            }
            Step::Skip {
                archive,
                files,
                message,
            } => {
                json!({ "kind": "skip", "archive": archive, "files": files, "message": message })
            }
            Step::Failed(f) => json!({
                "kind": "failed",
                "reason": f.kind.name(),
                "message": f.message,
            }),
        }
        .to_string()
    }

    /// How far it is, as JSON: the stage, which archive of how many, and the stage's bytes.
    #[must_use]
    pub fn progress(&self) -> String {
        let p = self.fetch.progress();
        json!({
            "stage": p.stage.name(),
            "archive": p.archive,
            "archives": p.archives,
            "done": p.done,
            "total": p.total,
        })
        .to_string()
    }

    pub fn api_reply(&mut self, body: Option<&str>, error: Option<&str>) {
        let reply = match body {
            Some(b) => Ok(b),
            None => Err(error.unwrap_or("no answer")),
        };
        self.fetch.api_reply(reply);
    }

    /// What a connection of a download step does next, as JSON: `range` (`id`, `url`, `from`,
    /// `to`), `wait` or `finished`.
    pub fn next_range(&mut self) -> String {
        match self.fetch.next_range() {
            Next::Range { id, url, from, to } => {
                json!({ "kind": "range", "id": id, "url": url, "from": from, "to": to })
            }
            Next::Wait => json!({ "kind": "wait" }),
            Next::Finished => json!({ "kind": "finished" }),
        }
        .to_string()
    }

    /// A piece of range `id`, decrypted in place: what to write in the archive now, each
    /// `(offset, bytes)`, or `None` to stop this connection.
    pub fn range_piece(&mut self, id: usize, bytes: &mut [u8]) -> Option<Vec<(u64, Vec<u8>)>> {
        self.fetch.range_piece(id, bytes)
    }

    /// Range `id` ended: `status` is the HTTP status it answered (200 when it ran to its end), or
    /// `error` what failed.
    pub fn range_ended(&mut self, id: usize, status: u16, error: Option<&str>) {
        let end = match (error, status) {
            (Some(e), _) => DownloadEnd::Failed(e.to_owned()),
            (None, 200) => DownloadEnd::Ended,
            (None, s) => DownloadEnd::Status(s),
        };
        self.fetch.range_ended(id, &end);
    }

    /// The bytes a read step asked for: what to write, `(file, offset, bytes)`.
    pub fn read(&mut self, bytes: &[u8]) -> Vec<(String, u64, Vec<u8>)> {
        self.fetch
            .read(bytes)
            .into_iter()
            .map(|w| (w.file, w.offset, w.bytes))
            .collect()
    }

    pub fn done(&mut self) {
        self.fetch.done();
    }

    /// The set's files, written and checked by their hashes, read as the launcher reads a set:
    /// they must report the iterations the set is listed with. `files` are each file's name, size
    /// and bytes.
    ///
    /// # Errors
    /// Why not, in words for the player.
    pub fn check(
        &self,
        files: Vec<(String, u64, Box<dyn dereth_dat::DatStorage>)>,
    ) -> Result<(), String> {
        let read: Vec<_> = files
            .into_iter()
            .filter_map(|(name, size, storage)| {
                datset::file_in_storage(self.kind, &name, size, storage)
            })
            .collect();
        downloads::check(self.fetch.set(), &read)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the web page's view of getting a data set: tooling around the client).
    use super::*;
    use dereth_launch::dat::testdat;

    /// The page's list names each set with what it costs and where it is kept, by its game.
    #[test]
    fn offers_list_each_set_with_its_sizes_and_folder() {
        let v: serde_json::Value = serde_json::from_str(&offers()).unwrap();
        assert_eq!(v["guide"], "https://www.accpp.net/manual-installation");
        let eor = &v["sets"][0];
        assert_eq!(eor["id"], "end-of-retail");
        assert_eq!(eor["kind"], "modern");
        assert_eq!(eor["download"], 507_898_112u64 + 54_808_779);
        assert_eq!(
            eor["size"],
            926_941_184u64 + 348_127_232 + 1_048_576 + 133_169_152
        );
        assert_eq!(eor["folder"], "sets/end-of-retail");
        assert_eq!(eor["archives"][0], "end-of-retail-1.zip");
        let sets = v["sets"].as_array().unwrap();
        let by = |id: &str| sets.iter().find(|s| s["id"] == id).unwrap();
        assert_eq!(by("launch")["kind"], "classic");
        assert_eq!(by("auberean-end-of-retail")["game"], "auberean");
        assert_eq!(
            by("auberean-end-of-retail")["folder"],
            "auberean/sets/auberean-end-of-retail"
        );
    }

    /// The first step asks the archive host's API about the set's first archive.
    #[test]
    fn the_first_step_asks_the_host() {
        let mut g = GetDats::begin("end-of-retail", &[]).unwrap();
        let step: serde_json::Value = serde_json::from_str(&g.step()).unwrap();
        assert_eq!(step["kind"], "api");
        assert!(step["url"]
            .as_str()
            .unwrap()
            .starts_with("https://g.api.mega.co.nz/cs?id="));
        assert_eq!(step["body"], r#"[{"a":"g","g":1,"n":"X0ciAbpK","ssl":2}]"#);
        assert!(GetDats::begin("no-such-set", &[]).is_err());
    }

    /// The host's download limit is a failure of its own kind.
    #[test]
    fn a_spent_download_limit_says_so() {
        let mut g = GetDats::begin("end-of-retail", &[]).unwrap();
        g.step();
        g.api_reply(Some("[-17]"), None);
        let step: serde_json::Value = serde_json::from_str(&g.step()).unwrap();
        assert_eq!(step["kind"], "failed");
        assert_eq!(step["reason"], "over-quota");
    }

    fn storage(bytes: Vec<u8>) -> Box<dyn dereth_dat::DatStorage> {
        Box::new(bytes)
    }

    /// The files must report the set's iterations, read as the launcher reads a set's files.
    #[test]
    fn the_files_must_report_the_sets_iterations() {
        let g = GetDats::begin("launch", &[]).unwrap();
        let portal = testdat::pre_tod(false, 102);
        let cell = testdat::pre_tod(true, 67);
        let files = |p: Vec<u8>, c: Vec<u8>| {
            vec![
                ("portal.dat".to_owned(), p.len() as u64, storage(p)),
                ("cell.dat".to_owned(), c.len() as u64, storage(c)),
            ]
        };
        assert_eq!(g.check(files(portal.clone(), cell.clone())), Ok(()));
        let later = testdat::pre_tod(true, 1593);
        let e = g.check(files(portal.clone(), later)).unwrap_err();
        assert!(e.contains("1593"), "{e}");
        let e = g
            .check(vec![(
                "portal.dat".to_owned(),
                portal.len() as u64,
                storage(portal),
            )])
            .unwrap_err();
        assert!(e.contains("cell"), "{e}");
    }
}
