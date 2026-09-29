//! Vectors: empyrean/fixtures/vectors/entity/numerics_* and binary_reader_read_string16l
//! System.Numerics quaternion/vector ops and BinaryReader primitives/ReadString16L are bit-exact
//! with net10 vectors.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use empyrean_common::dotnet::binary_reader::ReadError;
use empyrean_common::dotnet::{BinaryReader, Quaternion, Vector3};
use empyrean_common::vectors::{self, f32_of, same_f32, Case};
use serde_json::Value;

/// Collects mismatches over a file and fails once, listing them.
struct Report {
    name: &'static str,
    total: usize,
    failures: Vec<String>,
}

impl Report {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            total: 0,
            failures: Vec::new(),
        }
    }

    fn check(&mut self, case: &Case, ok: bool, got: impl std::fmt::Debug) {
        self.total += 1;
        if !ok {
            self.failures.push(format!(
                "in {} expected {} got {got:?}",
                case.input, case.output
            ));
        }
    }

    fn finish(self) {
        assert!(self.total > 0, "{}: no cases ran", self.name);
        if !self.failures.is_empty() {
            let shown: Vec<_> = self.failures.iter().take(20).cloned().collect();
            panic!(
                "{}: {} of {} cases differ from net10:\n  {}",
                self.name,
                self.failures.len(),
                self.total,
                shown.join("\n  ")
            );
        }
    }
}

fn f(v: &Value) -> f32 {
    f32_of(v).unwrap_or_else(|| panic!("not a float: {v}"))
}

fn quat(v: &Value) -> Quaternion {
    Quaternion::new(f(&v[0]), f(&v[1]), f(&v[2]), f(&v[3]))
}

fn vec3(v: &Value) -> Vector3 {
    Vector3::new(f(&v[0]), f(&v[1]), f(&v[2]))
}

fn same_quat(a: Quaternion, b: Quaternion) -> bool {
    same_f32(a.x, b.x) && same_f32(a.y, b.y) && same_f32(a.z, b.z) && same_f32(a.w, b.w)
}

fn same_vec3(a: Vector3, b: Vector3) -> bool {
    same_f32(a.x, b.x) && same_f32(a.y, b.y) && same_f32(a.z, b.z)
}

/// `Quaternion.CreateFromAxisAngle` scales the axis as given by the half angle's sine and takes
/// its cosine as `W`, and its one sine-cosine call gives the same bits as a sine then a cosine:
/// the form the spell and missile orientations were computed with before they shared it.
#[test]
fn create_from_axis_angle_is_the_half_angle_sine_and_cosine_bit_for_bit() {
    use empyrean_common::math::{cosf, sinf};
    let axes = [
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(1.0, 0.0, 0.0),
        // Not a unit axis: it is not normalised.
        Vector3::new(3.0, -4.0, 12.0),
        Vector3::new(-0.0, 1e-30, f32::MAX),
    ];
    let mut n = 0;
    for step in -1500..=1500 {
        #[allow(clippy::cast_precision_loss)]
        let angle = step as f32 * 0.0137;
        for axis in axes {
            let got = Quaternion::create_from_axis_angle(axis, angle);
            let h = angle * 0.5;
            let (s, c) = (sinf(h), cosf(h));
            let want = Quaternion::new(axis.x * s, axis.y * s, axis.z * s, c);
            assert!(
                same_quat(got, want),
                "{axis:?} {angle}: {got:?} vs {want:?}"
            );
            n += 1;
        }
    }
    for angle in [
        0.0,
        -0.0,
        std::f32::consts::PI,
        1e30,
        f32::NAN,
        f32::INFINITY,
    ] {
        let got = Quaternion::create_from_axis_angle(axes[2], angle);
        let h = angle * 0.5;
        let (s, c) = (sinf(h), cosf(h));
        let want = Quaternion::new(3.0 * s, -4.0 * s, 12.0 * s, c);
        assert!(same_quat(got, want), "{angle}: {got:?} vs {want:?}");
    }
    assert!(n > 10_000);
}

fn replay(name: &'static str, mut one: impl FnMut(&Case, &mut Report)) {
    let file = vectors::load_named("entity", name);
    let mut report = Report::new(name);
    for case in &file.cases {
        one(case, &mut report);
    }
    report.finish();
}

#[test]
fn create_from_yaw_pitch_roll_matches_net10() {
    replay("numerics_create_from_yaw_pitch_roll", |case, r| {
        let i = &case.input;
        let (yaw, pitch, roll) = (f(&i["yaw"]), f(&i["pitch"]), f(&i["roll"]));
        let got = Quaternion::create_from_yaw_pitch_roll(yaw, pitch, roll);
        r.check(
            case,
            same_quat_on_host(got, quat(&case.output), [yaw, pitch, roll]),
            got,
        );
    });
}

/// Same quat on host.
fn same_quat_on_host(got: Quaternion, want: Quaternion, angles: [f32; 3]) -> bool {
    if same_quat(got, want) {
        return true;
    }
    let host_libm = (empyrean_common::math::PORTABLE || !cfg!(windows))
        && angles.iter().any(|a| (a * 0.5).abs() >= 5.0e6);
    let near = |a: f32, b: f32| (a.to_bits() as i64 - b.to_bits() as i64).abs() <= 2;
    host_libm
        && near(got.x, want.x)
        && near(got.y, want.y)
        && near(got.z, want.z)
        && near(got.w, want.w)
}

/// The rolls where net10 and the .NET 8 `MathF.Sin`/`MathF.Cos` formula disagree (a scan of about
/// 2^28 floats found them), each with yaw and pitch that force every `Vector128.SinCos` branch.
#[test]
fn create_from_yaw_pitch_roll_rare_cases_match_net10() {
    replay("numerics_create_from_yaw_pitch_roll_rare", |case, r| {
        let i = &case.input;
        let (yaw, pitch, roll) = (f(&i["yaw"]), f(&i["pitch"]), f(&i["roll"]));
        let got = Quaternion::create_from_yaw_pitch_roll(yaw, pitch, roll);
        r.check(
            case,
            same_quat_on_host(got, quat(&case.output), [yaw, pitch, roll]),
            got,
        );
    });
}

#[test]
fn create_from_roll_matches_net10() {
    replay("numerics_create_from_roll", |case, r| {
        let got = Quaternion::create_from_yaw_pitch_roll(0.0, 0.0, f(&case.input["roll"]));
        r.check(case, same_quat(got, quat(&case.output)), got);
    });
}

#[test]
fn quaternion_multiply_matches_net10() {
    replay("numerics_quaternion_multiply", |case, r| {
        let got = quat(&case.input["a"]) * quat(&case.input["b"]);
        r.check(case, same_quat(got, quat(&case.output)), got);
    });
}

#[test]
fn quaternion_normalize_and_length_match_net10() {
    replay("numerics_quaternion_normalize", |case, r| {
        let q = quat(&case.input["q"]);
        let got = (Quaternion::normalize(q), q.length(), q.length_squared());
        let o = &case.output;
        let ok =
            same_quat(got.0, quat(&o[0])) && same_f32(got.1, f(&o[1])) && same_f32(got.2, f(&o[2]));
        r.check(case, ok, got);
    });
}

#[test]
fn vector3_transform_matches_net10() {
    replay("numerics_vector3_transform", |case, r| {
        let got = Vector3::transform(vec3(&case.input["v"]), quat(&case.input["q"]));
        r.check(case, same_vec3(got, vec3(&case.output)), got);
    });
}

#[test]
fn vector3_normalize_and_length_match_net10() {
    replay("numerics_vector3_normalize", |case, r| {
        let v = vec3(&case.input["v"]);
        let got = (Vector3::normalize(v), v.length(), v.length_squared());
        let o = &case.output;
        let ok =
            same_vec3(got.0, vec3(&o[0])) && same_f32(got.1, f(&o[1])) && same_f32(got.2, f(&o[2]));
        r.check(case, ok, got);
    });
}

/// V232: `ReadString16L` reads the client's packed string as the client
/// writes it, so over every input of ACE's .NET vectors it agrees with `dereth-protocol`'s reader: the
/// same text and position where that succeeds, a failure where it fails. (The vectors' expected
/// values are .NET's UTF-8 reading, which the ruling retired; their inputs remain a good sweep.)
#[test]
fn read_string16l_reads_the_clients_packed_string() {
    replay("binary_reader_read_string16l", |case, r| {
        let bytes: Vec<u8> = case.input["bytes"]
            .as_array()
            .expect("bytes")
            .iter()
            .map(|b| u8::try_from(b.as_u64().expect("byte")).expect("byte"))
            .collect();
        let mut reader = BinaryReader::new(&bytes);
        let got = reader.read_string16l();
        // dereth-protocol requires the padding bytes to be present; the server, like ACE, skips past the
        // end of the message, so a success is compared against dereth-protocol on the padded buffer.
        let want = dereth_protocol::Reader::new(&bytes).pstring();
        let mut padded = bytes.clone();
        padded.extend([0u8; 4]);
        let mut theirs = dereth_protocol::Reader::new(&padded);
        let want_padded = theirs.pstring();
        let ok = match (&got, &want_padded) {
            (Ok(a), Ok(b)) => a == b && reader.position() == theirs.position(),
            (Err(_), _) => want.is_err(),
            (Ok(_), Err(_)) => false,
        };
        r.check(case, ok, (got, reader.position(), want));
    });

    // Accented text and the curly quote are CP-1252 bytes, and a following field still reads.
    let data = [5u8, 0, b'c', b'a', b'f', 0xE9, 0x92, 0, 7, 0, 0, 0];
    let mut r = BinaryReader::new(&data);
    assert_eq!(r.read_string16l().as_deref(), Ok("caf\u{e9}\u{2019}"));
    assert_eq!(r.read_u32(), Ok(7));
}

/// Hand cases for the reads the vectors do not cover: fixed-size reads fail without moving,
/// `ReadBytes` returns what is left, `ReadChars` reads byte by byte while a sequence is pending, and
/// `ReadString32L` drops its length prefix bytes.
#[test]
fn binary_reader_primitives_follow_dotnet() {
    let data = [1u8, 0, 0, 0, 0x00, 0x00, 0x80, 0x3F, 9];
    let mut r = BinaryReader::new(&data);
    assert_eq!(r.read_u32(), Ok(1));
    assert_eq!(r.read_f32(), Ok(1.0));
    assert_eq!(
        r.read_u16(),
        Err(ReadError::EndOfStream {
            at: 8,
            needed: 2,
            available: 1
        })
    );
    assert_eq!(r.position(), 8, "a failed read does not move");
    assert_eq!(r.read_bytes(5), &[9]);
    r.skip(10);
    assert_eq!((r.position(), r.remaining(), r.rest()), (19, 0, &[][..]));
    assert_eq!(
        r.read_byte(),
        Err(ReadError::EndOfStream {
            at: 19,
            needed: 1,
            available: 0
        })
    );

    let mut r = BinaryReader::new(&[0xC3, 0xA9, 0x41]);
    assert_eq!((r.read_chars(1).as_deref(), r.position()), (Ok("é"), 2));
    // An incomplete sequence at the end of the stream is dropped, not replaced.
    let mut r = BinaryReader::new(&[0x41, 0xC3]);
    assert_eq!((r.read_chars(2).as_deref(), r.position()), (Ok("A"), 2));

    // ReadString32L (V232): u32 byte count 4, one length byte (3), "abc"; the field is 8 bytes.
    let data = [4u8, 0, 0, 0, 3, b'a', b'b', b'c', 0];
    let mut r = BinaryReader::new(&data);
    assert_eq!(r.read_string32l().as_deref(), Ok("abc"));
    assert_eq!(r.position(), 8);
    let mut r = BinaryReader::new(&[0u8, 0, 0, 0]);
    assert_eq!(r.read_string32l().as_deref(), Ok(""));
    let mut r = BinaryReader::new(&[0xFFu8, 0xFF, 0xFF, 0xFF, 0, 0]);
    assert!(matches!(
        r.read_string32l(),
        Err(ReadError::EndOfStream { .. })
    ));
    // 130 characters: the client's two-byte length starts at 128 (ACE switched at 256).
    let mut data = vec![132u8, 0, 0, 0, 0x80, 130];
    data.extend(std::iter::repeat_n(b'x', 130));
    let mut r = BinaryReader::new(&data);
    assert_eq!(r.read_string32l(), Ok("x".repeat(130)));
}
