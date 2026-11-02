use auberean_primitives::vlong::Vlong;

use super::oracle::check;
use super::vlong_cases;

fn number(words: &[u32], negative: u32) -> Vlong {
    let mut v = Vlong::new(0);
    v.load(&words[1..=words[0] as usize]);
    v.with_sign(negative != 0)
}

fn packed(v: &Vlong) -> Vec<u32> {
    assert_eq!(v.pack().len(), v.pack_size());
    v.pack()
        .chunks(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Numbers of any size made from a word or from words, multiplied (the product cut to the sum
/// of the factors' bit lengths), added across signs (the smaller magnitude taken from the
/// larger) and written in their binary form answer as the September 2013 client's own code
/// does, word for word.
///
/// Behaviour: geometry.vlong.every-operation-answers-as-the-client-does
#[test]
fn every_vlong_operation_answers_as_the_client_does() {
    check(vlong_cases::CASES, |label, _, inp| {
        let out = match label {
            "new" => packed(&Vlong::new(inp[0])),
            "load" => packed(&number(inp, 0)),
            "mul" | "add" => {
                let na = inp[0] as usize + 1;
                let nb = inp[na] as usize + 1;
                let (sa, sb) = (inp[na + nb], inp[na + nb + 1]);
                let a = number(&inp[..na], sa);
                let b = number(&inp[na..na + nb], sb);
                let r = if label == "mul" {
                    a.mul(&b)
                } else {
                    let mut a = a;
                    a.add_assign(&b);
                    a
                };
                [vec![u32::from(r.is_negative())], packed(&r)].concat()
            }
            _ => return None,
        };
        Some(out)
    });
}

/// A string of hexadecimal digits reads as the number it writes, any character that is not
/// a digit counting as 0 and a nul ending it; the binary form reads back as the number, and a
/// form shorter than its count promises is refused.
///
/// Behaviour: geometry.vlong.hex-strings-and-the-binary-form-read-back
#[test]
fn hex_strings_and_the_binary_form_read_back() {
    let mut v = Vlong::new(7);
    assert!(v.read_hex(b"123456789abcdef0123"));
    assert_eq!(v.words(), &[0xcdef_0123, 0x4567_89ab, 0x123][..]);
    v.read_hex(b"fFz1\0ff");
    assert_eq!(v.words(), &[0xff01]);
    let mut back = Vlong::new(0);
    let mut big = Vlong::new(0);
    big.read_hex(b"8000000000000000000000000000000000000001");
    assert_eq!(back.unpack(&big.pack()), Some(24));
    assert_eq!(back, big);
    assert_eq!(back.unpack(&[5, 0, 0, 0, 1, 0, 0, 0]), None);
}
