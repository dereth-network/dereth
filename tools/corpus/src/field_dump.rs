//! Write decoded fields from the shared corpus for the census tool.

use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_protocol::Message;
use std::fmt::Debug;
use std::io::Write;

#[cfg(test)]
mod tests;

enum Decoded {
    Ok(String),
    Trailing(String, usize),
    NoDecoder,
    AceZeroBlob,
    Failed(String),
}

fn d<M: Message + Debug>(body: &[u8]) -> Decoded {
    match dereth_protocol::read_body_padded::<M>(body) {
        Ok(m) => Decoded::Ok(format!("{m:#?}")),
        Err(e) => {
            let mut r = dereth_protocol::Reader::body(body);
            match M::read(&mut r) {
                Ok(m) => Decoded::Trailing(format!("{m:#?}"), r.remaining()),
                Err(_) => Decoded::Failed(format!("{e:?}")),
            }
        }
    }
}

macro_rules! decode_field {
    ($dir:ident, $op:ident, $body:ident, $ty:ty, skip) => {};
    ($dir:ident, $op:ident, $body:ident, $ty:ty, both) => {
        if $op == <$ty as Message>::OPCODE.0 {
            return d::<$ty>($body);
        }
    };
    ($dir:ident, $op:ident, $body:ident, $ty:ty, C2s) => {
        if matches!($dir, Direction::ClientToServer) && $op == <$ty as Message>::OPCODE.0 {
            return d::<$ty>($body);
        }
    };
    ($dir:ident, $op:ident, $body:ident, $ty:ty, S2c) => {
        if matches!($dir, Direction::ServerToClient) && $op == <$ty as Message>::OPCODE.0 {
            return d::<$ty>($body);
        }
    };
}

macro_rules! field_decoders {
    (messages { $( $m:ident :: $t:ident, $ty:ty, $capture:ident, $rank:tt, $fields:ident; )* }
     opaque { $( $om:ident :: $ot:ident, $oty:ty, $orank:tt, $ofields:ident; )* }) => {
        fn decode(dir: Direction, op: u32, body: &[u8]) -> Decoded {
            $( decode_field!(dir, op, body, $ty, $fields); )*
            $( decode_field!(dir, op, body, $oty, $ofields); )*
            if op == 0 && body.iter().all(|&b| b == 0) { Decoded::AceZeroBlob } else { Decoded::NoDecoder }
        }
    };
}

dereth_protocol::for_each_message!(field_decoders);

fn derived(op: u32, body: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if op == 0xF61C {
        if let Ok(m) = dereth_protocol::read_body_padded::<
            dereth_protocol::movement::MovementMoveToState,
        >(body)
        {
            if let Ok(f) = m.0.raw_motion_state.flags() {
                out.push(("raw_motion_state.flags".to_string(), format!("0x{f:X}")));
            }
            out.push((
                "raw_motion_state.actions.len".to_string(),
                format!("{}", m.0.raw_motion_state.actions.len()),
            ));
        }
    }
    out
}

fn extra_records(op: u32, body: &[u8]) -> Vec<(&'static str, Decoded)> {
    let mut out = Vec::new();
    if op == 0xF74C {
        if let Ok(m) = dereth_protocol::read_body_padded::<
            dereth_protocol::movement::MovementSetObjectMovement,
        >(body)
        {
            out.push((
                "MovementBuffer",
                match m.decoded_movement() {
                    Ok(b) => Decoded::Ok(format!("{b:#?}")),
                    Err(e) => Decoded::Failed(format!("{e:?}")),
                },
            ));
        }
    }
    out
}

fn split(b: &CorpusBlob) -> (&'static str, u32, &[u8]) {
    let p = &b.payload;
    if b.opcode == dereth_protocol::OrderedActionHeader::MAGIC && p.len() >= 12 {
        (
            "action",
            u32::from_le_bytes([p[8], p[9], p[10], p[11]]),
            &p[12..],
        )
    } else if b.opcode == dereth_protocol::OrderedEventHeader::MAGIC && p.len() >= 16 {
        (
            "event",
            u32::from_le_bytes([p[12], p[13], p[14], p[15]]),
            &p[16..],
        )
    } else {
        ("bare", b.opcode, if p.len() >= 4 { &p[4..] } else { &[] })
    }
}

fn name_of(op: u32) -> &'static str {
    dereth_protocol::Opcode(op).name().unwrap_or("?")
}

pub(crate) fn run() {
    // The dump is this run's output: `corpus_census.txt` in cargo's target directory, the folder
    // above the profile folder that holds this executable.
    let out_path = std::env::var("DERETH_TEST_CENSUS_OUT").unwrap_or_else(|_| {
        std::env::current_exe()
            .ok()
            .and_then(|exe| Some(exe.parent()?.parent()?.join("corpus_census.txt")))
            .unwrap_or_else(|| std::env::temp_dir().join("corpus_census.txt"))
            .to_string_lossy()
            .into_owned()
    });
    if let Some(dir) = std::path::Path::new(&out_path).parent() {
        std::fs::create_dir_all(dir).expect("the output directory must be creatable");
    }
    let f = std::fs::File::create(&out_path).expect("the dump must be writable");
    let mut w = std::io::BufWriter::new(f);

    let (mut s2c, mut c2s) = (0usize, 0usize);
    let (mut ok, mut nodec, mut failed, mut trailing) = (0usize, 0usize, 0usize, 0usize);
    let mut zero_blob = 0usize;
    let mut ran = 0usize;
    for corpus in Corpus::shared_all() {
        let scen = &corpus.name;
        ran += 1;
        for b in &corpus.blobs {
            match b.dir {
                Direction::ServerToClient => s2c += 1,
                Direction::ClientToServer => c2s += 1,
            }
            let (wrap, op, body) = split(b);
            let dir = match b.dir {
                Direction::ServerToClient => "s2c",
                Direction::ClientToServer => "c2s",
            };
            let dec = decode(b.dir, op, body);
            let mut left_over = None;
            let (status, text) = match &dec {
                Decoded::Ok(s) => {
                    ok += 1;
                    ("ok", s.as_str())
                }
                Decoded::Trailing(s, n) => {
                    trailing += 1;
                    left_over = Some(*n);
                    ("ok-trailing", s.as_str())
                }
                Decoded::NoDecoder => {
                    nodec += 1;
                    ("no-decoder", "")
                }
                Decoded::AceZeroBlob => {
                    zero_blob += 1;
                    ("ace-zero-blob", "")
                }
                Decoded::Failed(s) => {
                    failed += 1;
                    ("failed", s.as_str())
                }
            };
            let mut extra = derived(op, body)
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(" | ");
            if let Some(n) = left_over {
                if !extra.is_empty() {
                    extra.push_str(" | ");
                }
                extra.push_str(&format!("trailing_bytes={n}"));
            }
            writeln!(
                w,
                "### {scen} {} {dir} {wrap} 0x{op:04X} {} {status}{}{extra}",
                b.idx,
                name_of(op),
                if extra.is_empty() { "" } else { " " }
            )
            .expect("write");
            for line in text.lines() {
                assert!(
                    !line.starts_with("###"),
                    "a Debug body produced a line that looks like a record header; the dump \
                     format is ambiguous and every count taken from it is suspect"
                );
                writeln!(w, "{line}").expect("write");
            }
            for (tag, sub) in extra_records(op, body) {
                let (status, text) = match &sub {
                    Decoded::Ok(s) => ("ok", s.as_str()),
                    Decoded::Trailing(s, _) => ("ok-trailing", s.as_str()),
                    Decoded::NoDecoder => ("no-decoder", ""),
                    Decoded::AceZeroBlob => ("ace-zero-blob", ""),
                    Decoded::Failed(s) => ("failed", s.as_str()),
                };
                writeln!(
                    w,
                    "### {scen} {} {dir} sub 0x{op:04X} {tag} {status}",
                    b.idx
                )
                .expect("write");
                for line in text.lines() {
                    writeln!(w, "{line}").expect("write");
                }
            }
        }
    }
    w.flush().expect("flush");

    eprintln!(
        "corpus census: {ran} scenarios | {s2c} server + {c2s} client blobs | {ok} decoded, \
         {trailing} decoded with bytes left over, {nodec} with no decoder, {zero_blob} ACE zero \
         blob, {failed}          failed to decode | -> {out_path}"
    );
    assert!(
        ran > 0 && s2c > 0 && c2s > 0,
        "the corpus must contain messages in both directions"
    );
    assert!(ok + trailing > 0, "the decoder must produce fields");
}
