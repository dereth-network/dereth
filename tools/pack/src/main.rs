//! `dereth-pack`: pack pictures into a client layer, as a manifest describes it.
//!
//! ```text
//! dereth-pack <manifest> [--out <container>]   write the container
//! dereth-pack <manifest> --check               say whether the container is current
//! ```
//!
//! The manifest's format is the library's ([`dereth_pack`]).

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut manifest = None;
    let mut out = None;
    let mut check = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--check" => check = true,
            "--out" => out = args.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!("dereth-pack <manifest> [--out <container>] | <manifest> --check");
                return ExitCode::SUCCESS;
            }
            _ if manifest.is_none() => manifest = Some(PathBuf::from(a)),
            _ => {
                eprintln!("dereth-pack: unexpected argument {a}");
                return ExitCode::FAILURE;
            }
        }
    }
    let Some(manifest) = manifest else {
        eprintln!("dereth-pack: no manifest named (dereth-pack --help)");
        return ExitCode::FAILURE;
    };
    if check {
        return match dereth_pack::is_current(&manifest) {
            Ok(true) => {
                println!("{}: the container is current", manifest.display());
                ExitCode::SUCCESS
            }
            Ok(false) => {
                eprintln!(
                    "{}: the container is not what the manifest makes; run dereth-pack {}",
                    manifest.display(),
                    manifest.display()
                );
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("dereth-pack: {e}");
                ExitCode::FAILURE
            }
        };
    }
    match dereth_pack::pack(&manifest, out.as_deref()) {
        Ok(packed) => {
            println!("{}: {} records", packed.out.display(), packed.ids.len());
            for id in &packed.flattened {
                println!(
                    "  {:#010X}: transparent pixels laid on the background",
                    id.raw()
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("dereth-pack: {e}");
            ExitCode::FAILURE
        }
    }
}
