//! Strings compared, ordered, hashed, trimmed, replaced in, scrambled and appended to, as the
//! client's string natives do it, and the case-insensitive string's natives bound to the same work.
//! Fixture: each native called as the machine calls one on strings made for it.

use auberean_script::native_object::NativeObject;
use auberean_script::value::{join, split, NULL};
use auberean_script::Status;

use crate::call::Bench;

const TEXT: u32 = 0x76;
const CI_TEXT: u32 = 0x1ACF;

/// The string's units as text.
fn s(b: &Bench, id: u32) -> String {
    String::from_utf16(&b.text_of(id)).expect("text")
}

/// **A string orders against another as the C library compares them**: with case kept, -1, 0 or 1
/// and the orderings from it; with case ignored (ASCII letters lowered), the difference of the
/// first units that differ. Its hash is the client's: each unit added to the hash shifted left four
/// bits, the top nibble folded back. Trimming takes space, tab, line feed, carriage return and form
/// feed off both ends; replacing replaces every occurrence, searching on after each, and answers
/// how many; scrambling swaps each byte's halves and keys each whole pair of units, and
/// unscrambling undoes it; a 64-bit integer and a double append in decimal, the double with six
/// decimals. The case-insensitive string's natives do the same.
///
/// Behaviour: script.glue.strings-ordered-hashed-trimmed-replaced-scrambled
#[test]
fn strings_order_hash_trim_replace_and_scramble_as_the_client_does() {
    let mut b = Bench::new();
    let (abc, abd, hello, help) = (
        b.text("abc"),
        b.text("ABD"),
        b.text("Hello"),
        b.text("help"),
    );

    // Compare (0x10): case kept (fp[-3] set) gives -1, 0, 1; ignored gives the difference.
    assert_eq!(b.answer(TEXT, 0x10, abc, &[abd, 1]), 1, "'a' after 'A'");
    assert_eq!(b.answer(TEXT, 0x10, abc, &[abd, 0]) as i32, -1, "'c' - 'd'");
    assert_eq!(
        b.answer(TEXT, 0x10, hello, &[help, 0]) as i32,
        -4,
        "'l' - 'p'"
    );
    assert_eq!(
        b.answer(TEXT, 0x10, hello, &[help, 1]) as i32,
        -1,
        "'H' before 'h'"
    );
    assert_eq!(b.answer(TEXT, 0x10, abc, &[abc, 1]), 0);
    // The orderings: lt 0x13, gt 0x14, le 0x15, ge 0x16.
    let order = |b: &mut Bench, n, x, y, keep| b.answer(TEXT, n, x, &[y, keep]) & 0xFF;
    assert_eq!(order(&mut b, 0x13, hello, help, 0), 1);
    assert_eq!(order(&mut b, 0x14, hello, help, 0), 0);
    assert_eq!(order(&mut b, 0x15, abc, abc, 1), 1);
    assert_eq!(order(&mut b, 0x16, abc, abd, 0), 0);
    assert_eq!(order(&mut b, 0x16, abc, abd, 1), 1);

    // Hash (0x01).
    let fox = b.text("The quick brown fox");
    assert_eq!(b.answer(TEXT, 0x01, abc, &[]), 0x6783);
    assert_eq!(b.answer(TEXT, 0x01, fox, &[]), 0x03BA_7E18);

    // Trim (0x0F).
    let padded = b.text(" \t\x0c hi there \r\n");
    assert_eq!(b.call(TEXT, 0x0F, padded, &[], 0).0, Status::Success);
    assert_eq!(s(&b, padded), "hi there");
    let blank = b.text(" \n ");
    b.call(TEXT, 0x0F, blank, &[], 0);
    assert_eq!(s(&b, blank), "");

    // Replace (0x22): the string at fp[-2] by the one at fp[-3].
    let (dash, colons, aa, one) = (b.text("-"), b.text("::"), b.text("aa"), b.text("b"));
    let abc_dashed = b.text("a-b-c");
    assert_eq!(b.answer(TEXT, 0x22, abc_dashed, &[dash, colons]), 2);
    assert_eq!(s(&b, abc_dashed), "a::b::c");
    let aaa = b.text("aaa");
    assert_eq!(b.answer(TEXT, 0x22, aaa, &[aa, one]), 1);
    assert_eq!(s(&b, aaa), "ba");
    assert_eq!(b.answer(TEXT, 0x22, aaa, &[dash, one]), 0);
    assert_eq!(s(&b, aaa), "ba");

    // Encrypt (0x1C) and decrypt (0x1D).
    let ab = b.text("ABC");
    b.call(TEXT, 0x1C, ab, &[], 0);
    assert_eq!(b.text_of(ab), [0x922C, 0xE5A0, 0x0034]);
    b.call(TEXT, 0x1D, ab, &[], 0);
    assert_eq!(s(&b, ab), "ABC");
    let long = b.text("A longer string to scramble, past sixteen words of key, and back again.");
    b.call(TEXT, 0x1C, long, &[], 0);
    assert_ne!(
        b.text_of(long)[..8],
        *"A longer".encode_utf16().collect::<Vec<_>>()
    );
    b.call(TEXT, 0x1D, long, &[], 0);
    assert_eq!(
        s(&b, long),
        "A longer string to scramble, past sixteen words of key, and back again."
    );

    // Append a 64-bit integer (0x20) and a double (0x21): fp[-3] low, fp[-2] high.
    let out = b.text("n=");
    let minus_five = (-5_i64) as u64;
    b.call(
        TEXT,
        0x20,
        out,
        &[split(minus_five).1, split(minus_five).0],
        0,
    );
    let x = 1.5_f64.to_bits();
    b.call(TEXT, 0x21, out, &[split(x).1, split(x).0], 0);
    assert_eq!(s(&b, out), "n=-51.500000");

    // The case-insensitive string's natives are the string's.
    let ci = b.object(
        CI_TEXT,
        NativeObject::Text("Hello".encode_utf16().collect()),
    );
    assert_eq!(b.answer(CI_TEXT, 0x11, ci, &[help, 0]) as i32, -4);
    assert_eq!(
        b.answer(CI_TEXT, 0x01, ci, &[]),
        b.answer(TEXT, 0x01, hello, &[])
    );
    let (l, upper_l) = (b.text("l"), b.text("L"));
    assert_eq!(b.answer(CI_TEXT, 0x18, ci, &[l, upper_l]), 2);
    assert_eq!(s(&b, ci), "HeLLo");

    // A null receiver ends the script.
    assert_eq!(b.call(TEXT, 0x10, NULL, &[abc, 0], 1).0, Status::Crash);
}

/// A text reads as a whole number as the C library reads one in base 0: white space and a sign
/// first, `0x` for hexadecimal and a leading `0` for octal, all of it read and in range (an
/// unsigned number takes a minus sign; the empty text counts, a sign or `0x` alone does not); a
/// path loses its extension at its last dot, or its folders up to its last separator, its wide
/// units narrowed on the way.
///
/// Behaviour: script.glue.text-utilities-read-numbers-and-cut-paths
#[test]
fn a_text_reads_as_a_whole_number_as_the_c_library_reads_it_and_paths_are_cut() {
    use auberean_glue::text::{
        reads_as_whole_number as whole, without_directory, without_extension,
    };
    let u = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
    for (t, i32_, u32_, i64_, u64_) in [
        ("12", true, true, true, true),
        ("  -7", true, true, true, true),
        ("+0x1F", true, true, true, true),
        ("017", true, true, true, true),
        ("08", false, false, false, false),
        ("", true, true, true, true),
        ("-", false, false, false, false),
        ("0x", false, false, false, false),
        ("12 ", false, false, false, false),
        ("2147483647", true, true, true, true),
        ("2147483648", false, true, true, true),
        ("-2147483648", true, true, true, true),
        ("-2147483649", false, true, true, true),
        ("4294967296", false, false, true, true),
        ("-4294967295", false, true, true, true),
        ("18446744073709551615", false, false, false, true),
        ("18446744073709551616", false, false, false, false),
        ("12\u{0}junk", true, true, true, true),
    ] {
        assert_eq!(
            (
                whole(&u(t), 32, true),
                whole(&u(t), 32, false),
                whole(&u(t), 64, true),
                whole(&u(t), 64, false)
            ),
            (i32_, u32_, i64_, u64_),
            "{t:?}"
        );
    }
    assert_eq!(
        without_extension(&u("dir.x\\file.tar.gz")),
        u("dir.x\\file.tar")
    );
    assert_eq!(without_extension(&u("a.b\\c")), u("a"));
    assert_eq!(without_extension(&u("plain")), u("plain"));
    assert_eq!(without_directory(&u("C:\\a/b\\c.txt")), u("c.txt"));
    assert_eq!(without_directory(&u("C:file")), u("file"));
    assert_eq!(without_directory(&u("\u{100}.txt")), u("?.txt"));

    // Through the natives: a byte answer below the string; a path changed in place.
    let mut b = crate::call::Bench::new();
    let n = b.text("0x10");
    assert_eq!(b.call(0x5D, 0x04, 0, &[n], 1).1[0].0, 1);
    let n = b.text("x10");
    assert_eq!(b.call(0x5D, 0x03, 0, &[n], 1).1[0].0, 0);
    let p = b.text("data\\file.txt");
    let (status, slots) = b.call_with_out(0x5D, 0x0A, 0, p, 1);
    assert_eq!((status, slots[0].0), (auberean_script::Status::Success, 1));
    assert_eq!(b.text_of(p), u("file.txt"));
    b.call_with_out(0x5D, 0x09, 0, p, 1);
    assert_eq!(b.text_of(p), u("file"));
}

/// **Two strings are equal or differ as the C library compares them**: case kept when the flag
/// (`fp[-3]`) is set, unit for unit; otherwise each unit lowered as the wide C library lowers it,
/// accented capitals too, and so are orderings with case ignored.
///
/// Behaviour: script.glue.strings-equal-with-case-kept-only-when-asked
#[test]
fn strings_are_equal_with_case_kept_only_when_asked() {
    let mut b = Bench::new();
    let (abc, upper_abc, e_acute, upper_e_acute) =
        (b.text("abc"), b.text("ABC"), b.text("é"), b.text("É"));
    let eq = |b: &mut Bench, x, y, keep| b.answer(TEXT, 0x11, x, &[y, keep]) & 0xFF;
    let ne = |b: &mut Bench, x, y, keep| b.answer(TEXT, 0x12, x, &[y, keep]) & 0xFF;
    assert_eq!(eq(&mut b, abc, upper_abc, 1), 0, "case kept");
    assert_eq!(eq(&mut b, abc, upper_abc, 0), 1, "case ignored");
    assert_eq!(eq(&mut b, abc, abc, 1), 1);
    assert_eq!(
        eq(&mut b, e_acute, upper_e_acute, 0),
        1,
        "an accented capital lowers"
    );
    assert_eq!(ne(&mut b, abc, upper_abc, 1), 1);
    assert_eq!(ne(&mut b, abc, upper_abc, 0), 0);
    assert_eq!(ne(&mut b, e_acute, upper_e_acute, 0), 0);
    // The case-insensitive string's equality (0x12) is the same native.
    assert_eq!(b.answer(CI_TEXT, 0x12, abc, &[upper_abc, 0]) & 0xFF, 1);
    // Ordering with case ignored lowers accented capitals too.
    assert_eq!(b.answer(TEXT, 0x10, e_acute, &[upper_e_acute, 0]), 0);
}

/// **A string reads as a 64-bit number as the C library reads one in base 0**: signed, clamped to
/// the least or greatest; unsigned, an overflowing magnitude is all ones, which a minus sign then
/// counts back to 1.
///
/// Behaviour: script.glue.strings-read-64-bit-numbers-the-c-librarys-way
#[test]
fn strings_read_64_bit_numbers_the_c_librarys_way() {
    let mut b = Bench::new();
    let mut read = |n: u32, s: &str| {
        let t = b.text(s);
        let (status, slots) = b.call(TEXT, n, t, &[], 2);
        assert_eq!(status, Status::Success);
        join(slots[1].0, slots[0].0)
    };
    let (signed, unsigned) = (0x19, 0x1A);
    assert_eq!(read(unsigned, "-18446744073709551616"), 1);
    assert_eq!(read(unsigned, "18446744073709551616"), u64::MAX);
    assert_eq!(read(unsigned, "-1"), u64::MAX);
    assert_eq!(read(unsigned, "0x10"), 16);
    assert_eq!(
        read(signed, "-18446744073709551616"),
        i64::MIN.cast_unsigned()
    );
    assert_eq!(
        read(signed, "9223372036854775808"),
        i64::MAX.cast_unsigned()
    );
    assert_eq!(read(signed, " -017"), (-15_i64).cast_unsigned());
}

/// Behaviour: script.glue.text-reads-as-a-floating-point-number
#[test]
fn a_text_reads_as_a_floating_point_number_as_the_c_library_reads_it() {
    use auberean_glue::text::reads_as_whole_double as reads;
    let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
    // Each case: the text, whether the client's library reads all of it without a range error.
    let cases: &[(&str, bool)] = &[
        ("", true),
        (" ", false),
        ("1", true),
        ("-1", true),
        ("+1", true),
        ("1.5", true),
        (".5", true),
        ("5.", true),
        ("-.5", true),
        (".", false),
        ("-", false),
        ("+-1", false),
        ("1e5", true),
        ("1E5", true),
        ("1d5", true),
        ("1D5", true),
        ("e5", false),
        (".e5", false),
        ("1.e5", true),
        ("1e", false),
        ("1e+", false),
        ("5e+", false),
        ("1e-5", true),
        ("1e0005", true),
        ("1e5.5", false),
        ("1.5.5", false),
        ("1e1e1", false),
        (" 1", true),
        ("\t\n\u{b}\u{c}\r1", true),
        ("\u{a0}1", true),
        ("\u{85}1", false),
        ("\u{3000}1", true),
        ("1 ", false),
        ("1,5", false),
        ("0x10", false),
        ("inf", false),
        ("nan", false),
        ("1\u{0}x", true),
        // Units the code page writes as a number's characters.
        ("\u{ff11}", true),
        ("1\u{ff10}", true),
        ("\u{2212}1", true),
        ("\u{221e}", true),
        ("\u{0661}", false),
        ("1\u{b2}", false),
        // The limits: overflow, and anything below the least normal double, are range errors.
        ("1e308", true),
        ("1.7976931348623157e308", true),
        ("1.7976931348623158e308", true),
        ("1.7976931348623159e308", false),
        ("1e309", false),
        ("-1e309", false),
        ("1e400", false),
        ("2.2250738585072014e-308", true),
        ("2.2250738585072013e-308", true),
        ("2.2250738585072012e-308", false),
        ("2.2250738585072011e-308", false),
        ("-2.2250738585072012e-308", false),
        ("1e-310", false),
        ("4.9406564584124654e-324", false),
        ("100e-325", false),
        ("1e-400", false),
        ("0e-400", true),
        ("0e2147483648", true),
        ("1e2147483648", false),
        ("1e-2147483649", false),
        // Only the first 21 significant digits count.
        ("2.2250738585072011360575e-308", false),
        ("2.225073858507201136059e-308", false),
        ("1.797693134862315807937e308", true),
        ("1.797693134862315807938e308", true),
        ("00000000000000000000000000000000000000001", true),
        ("1.00000000000000000000000000000000000001e308", true),
    ];
    for (t, want) in cases {
        assert_eq!(reads(&w(t)), *want, "{t:?}");
    }
    let tiny = format!("0.{}1e5000", "0".repeat(5000));
    assert!(reads(&w(&tiny)));
    let huge = format!("1{}e-5000", "0".repeat(5000));
    assert!(reads(&w(&huge)));
    assert!(!reads(&w(&format!("0.{}1", "0".repeat(400)))));
    assert!(!reads(&w(&format!("1{}", "0".repeat(400)))));
    // The native: a byte below the text, both precisions.
    let mut b = crate::call::Bench::new();
    for n in [0x07, 0x08] {
        let yes = b.text("2.5e3");
        let no = b.text("2.5e");
        assert_eq!(
            b.answer(0x5D, n, auberean_script::value::NULL, &[yes]) & 0xFF,
            1
        );
        assert_eq!(
            b.answer(0x5D, n, auberean_script::value::NULL, &[no]) & 0xFF,
            0
        );
    }
}

/// **The folder a script builds file names on is the empty name, and a folder's name is made one
/// way round**: the current folder answers 1 and empties the string; cleaning a folder's name
/// turns each backslash into a slash (or, asked for backslashes, each slash into a backslash),
/// narrows it, adds that separator when the name does not end in it, leaves an empty name empty,
/// and answers 1.
/// Fixture: each native called as the machine calls one, the string held in a local slot.
/// Behaviour: script.glue.folder-names-for-file-names
#[test]
fn the_current_folder_is_the_empty_name_and_folder_names_are_made_one_way_round() {
    const UTILS: u32 = 0x5D;
    let mut b = Bench::new();
    let held = b.text("C:\\AC2");
    let (status, slots) = b.call_with_local(UTILS, 1, NULL, held, &[None], 1);
    assert_eq!((status, slots[0].0), (Status::Success, 1));
    assert_eq!(s(&b, held), "");

    let mut clean = |text: &str, backslash: u32| {
        let held = b.text(text);
        let (status, slots) = b.call_with_local(UTILS, 2, NULL, held, &[None, Some(backslash)], 1);
        assert_eq!((status, slots[0].0), (Status::Success, 1));
        s(&b, held)
    };
    assert_eq!(clean("C:\\AC2\\logs", 0), "C:/AC2/logs/");
    assert_eq!(clean("C:/AC2/", 0), "C:/AC2/");
    assert_eq!(clean("C:/AC2\\x", 1), "C:\\AC2\\x\\");
    assert_eq!(clean("a\u{2212}", 0), "a?/");
    assert_eq!(clean("", 0), "");
    assert_eq!(clean("/", 0), "/");
}

/// **A string reference reads itself from a text tree by its children's keywords**: the table's
/// name found by its record (one no record has leaves the table), the token's text hashed as a
/// string's token and handed back, an unknown keyword warned of and passed over; a keyword with no
/// one text beneath it is an error, ends the reading and keeps what was read; no node reads as
/// nothing done.
///
/// Behaviour: script.glue.a-string-reference-reads-from-a-text-tree
#[test]
fn a_string_reference_reads_itself_from_a_text_tree_by_its_keywords() {
    use auberean_dat::tree::Node;
    use auberean_glue::text::{text_from_node, text_from_node_keeping_token};
    use auberean_script::native_object::TextInfo;

    const TABLE: u32 = 0x2500_0007;
    let mut names = |name: &str| if name == "strings.stb" { TABLE } else { 0 };

    let mut root = Node::new();
    root.init_file_info(b"t");
    root.make_child_text("STRINGTABLE")
        .make_child_text("strings.stb");
    root.make_child_text("something").make_child_text("else");
    root.make_child_text("stringtoken").make_child_text("Hello");
    let mut token = Vec::new();
    let mut t = TextInfo::default();
    assert!(text_from_node_keeping_token(
        &mut t,
        Some(&mut root),
        &mut token,
        &mut names
    ));
    assert_eq!(
        (t.table, t.string_id),
        (TABLE, auberean_assets::strings::token("Hello"))
    );
    assert_eq!(token, b"Hello");
    let info = root.file_info().expect("a root");
    assert_eq!((info.warnings, info.errors), (1, 0));

    // A name no record has leaves the table; an empty token stops the reading.
    let mut root = Node::new();
    root.init_file_info(b"t");
    root.make_child_text("STRINGTABLE").make_child_text("gone");
    root.make_child_text("STRINGTOKEN");
    root.make_child_text("STRINGTABLE")
        .make_child_text("strings.stb");
    let mut t = TextInfo {
        table: 5,
        ..TextInfo::default()
    };
    assert!(!text_from_node(&mut t, Some(&mut root), &mut names));
    assert_eq!(t.table, 5, "read as far as the error");
    assert_eq!(root.file_info().expect("a root").errors, 1);

    assert!(!text_from_node(&mut TextInfo::default(), None, &mut names));
}
