//! `empyrean-world-db`: a world pack's rows written out, a world of rows folded into a pack, and
//! the two compared.
//!
//! ```text
//! empyrean-world-db rows <world.pack> <empty folder> [--source <project> --revision <commit>]
//! empyrean-world-db pack <world.toml | folder of rows> <out.pack>
//! empyrean-world-db check <world.toml | folder of rows> <world.pack>
//! ```
//!
//! `rows` writes every record of a pack as rows, the same bytes for the same pack; `--source` and
//! `--revision` say where they came from. `pack` folds a world's layers (or reads one folder of
//! rows) and builds its pack, numbered by rule. `check` builds that pack and lists every record of
//! the given pack it doesn't hold field for field, ids aside; it fails on any.

use std::path::Path;
use std::process::ExitCode;

use empyrean_content::pack::Pack;
use empyrean_content::WorldContent;
use empyrean_world_db::compare::{dataset_of, differences};
use empyrean_world_db::files::{default_stamp, set_source};
use empyrean_world_db::{from_pack, number, read_dir, world, write, write_dir};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The world `at` names, folded and numbered: a `world.toml`'s layers, or one folder of rows.
fn world_at(at: &Path) -> Result<WorldContent, String> {
    let mut c = if at.is_file() {
        empyrean_world_db::world::fold_world(at)?
    } else {
        return world(&read_dir(at)?);
    };
    number(&mut c);
    Ok(c)
}

/// The world's pack, its dataset id from its rows written back.
fn pack_of(c: &WorldContent) -> Result<Vec<u8>, String> {
    let files = write(c, default_stamp())?;
    c.to_pack(dataset_of(&files))
        .map(|(bytes, _)| bytes)
        .map_err(|e| e.to_string())
}

fn run(args: &[String]) -> Result<bool, String> {
    let usage = "usage: empyrean-world-db rows <world.pack> <folder> [--source <project> --revision <commit>] | pack <world> <out.pack> | check <world> <world.pack>";
    match args {
        [cmd, pack, out, rest @ ..] if cmd == "rows" => {
            let p = Pack::open(Path::new(pack)).map_err(|e| e.to_string())?;
            let mut c = from_pack(&p).map_err(|e| e.to_string())?;
            number(&mut c);
            let mut files = write(&c, default_stamp())?;
            match rest {
                [] => {}
                [s, source, r, revision] if s == "--source" && r == "--revision" => {
                    set_source(&mut files, source, revision)?;
                }
                _ => return Err(usage.into()),
            }
            write_dir(Path::new(out), &files)?;
            println!("{} files written to {out}", files.len());
            Ok(true)
        }
        [cmd, at, out] if cmd == "pack" => {
            let bytes = pack_of(&world_at(Path::new(at))?)?;
            std::fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
            let p = Pack::from_bytes(bytes).map_err(|e| e.to_string())?;
            println!("{out}: content hash {}", hex(&p.header().content_hash));
            Ok(true)
        }
        [cmd, at, pack] if cmd == "check" => {
            let original = Pack::open(Path::new(pack)).map_err(|e| e.to_string())?;
            let rebuilt =
                Pack::from_bytes(pack_of(&world_at(Path::new(at))?)?).map_err(|e| e.to_string())?;
            let found = differences(&original, &rebuilt).map_err(|e| e.to_string())?;
            for line in &found {
                println!("{line}");
            }
            println!(
                "{pack} {}; the rows' pack {}; {} difference(s), ids aside",
                hex(&original.header().content_hash),
                hex(&rebuilt.header().content_hash),
                found.len()
            );
            Ok(found.is_empty())
        }
        _ => Err(usage.into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("empyrean-world-db: {e}");
            ExitCode::from(2)
        }
    }
}
