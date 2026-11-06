//! The text tree: the bracketed text files the client reads and writes for itself (the key maps
//! beside the program, the gameplay options a player exports).

use auberean_dat::tree::{self, number, parse::text_mode, Includes, Name, NameKind, Node, Parser};

fn names(n: &Node) -> Vec<String> {
    n.children()
        .iter()
        .map(|c| String::from_utf8_lossy(&c.name_text_now().unwrap_or_default()).into_owned())
        .collect()
}

fn parsed(text: &[u8]) -> Node {
    tree::parse_text(text).expect("parses")
}

fn written(text: &[u8]) -> String {
    let mut root = parsed(text);
    String::from_utf8(tree::to_text(&mut root)).expect("ASCII")
}

/// A key map in the shape the client saves it reads as a node a line: `VERSION` with its number,
/// then nameless nodes holding the name, the identity's eleven parts and each binding; each node
/// keeps the line and column its name began at, and the whole reads back to the same text.
///
/// Behaviour: data.text-tree.reads-as-the-client-parses
#[test]
fn a_saved_key_map_reads_as_nodes_and_writes_back_the_same() {
    let text = b"\nVERSION [ 1 ]\n\n\"\" [ mydefault ]\n\n\"\" [ 10409585 60644 24 240 236 24 0 80 242 63 34 ]\n\n\"\" [ EXAMINATION_PANEL_KEY DIKEYBOARD_X ]\n";
    let mut root = parsed(text);
    assert_eq!(root.name_text_now().as_deref(), Some(&b"-string-"[..]));
    assert_eq!(names(&root), ["VERSION", "", "", ""]);
    let v = root.child(0).expect("version");
    assert_eq!((v.line, v.column), (2, 1));
    assert_eq!(names(v), ["1"]);
    assert_eq!(
        (v.child(0).unwrap().line, v.child(0).unwrap().column),
        (2, 11)
    );
    assert_eq!(names(root.child(2).unwrap()).len(), 11);
    assert_eq!(root.child(2).unwrap().child(1).unwrap().column, 15);
    assert_eq!(
        names(root.child(3).unwrap()),
        ["EXAMINATION_PANEL_KEY", "DIKEYBOARD_X"]
    );
    assert_eq!(tree::to_text(&mut root), text.to_vec());
}

/// Braces and parentheses hold children as brackets do; a `#` line is a comment, its text after
/// the `#` kept with its line break; the comment lines before a node are that node's, blank lines
/// between them dropped (so `##` keeps one `#`); a comment after a node's name goes to the next
/// node, and one after the last node of a bracket or of the text to that node; the comment lines
/// at the very top, ended by a blank line, are the root's.
///
/// Behaviour: data.text-tree.reads-as-the-client-parses
#[test]
fn braces_and_comments_read_as_the_client_reads_them() {
    let root = parsed(
        b"{ MyKeys }\n\n# UI keys\n\n## panel keys\n{ TOOLBAR_PANEL_KEY     DIKEYBOARD_F12   }\n",
    );
    assert_eq!(names(&root), ["", ""]);
    assert_eq!(names(root.child(0).unwrap()), ["MyKeys"]);
    let b = root.child(1).unwrap();
    assert_eq!(b.comment(), b" UI keys\n# panel keys\n");
    assert_eq!(names(b), ["TOOLBAR_PANEL_KEY", "DIKEYBOARD_F12"]);

    let root =
        parsed(b"a # after a\nb\n# before c\nc [ x # after x\n y ] # after bracket\n# trailing\n");
    assert_eq!(names(&root), ["a", "b", "c"]);
    assert!(root.child(0).unwrap().comment().is_empty());
    assert_eq!(root.child(1).unwrap().comment(), b" after a\n");
    let c = root.child(2).unwrap();
    assert_eq!(c.comment(), b" before c\n after bracket\n trailing\n");
    assert_eq!(c.child(1).unwrap().comment(), b" after x\n");
    assert_eq!(
        (c.child(1).unwrap().line, c.child(1).unwrap().column),
        (5, 2)
    );

    let root = parsed(b"# header line\n# second\n\nfirst\n");
    assert_eq!(root.comment(), b" header line\n second\n");
    assert!(root.child(0).unwrap().comment().is_empty());
    assert_eq!(parsed(b"#\n#\nx\n").comment(), b"\n\n");
    assert_eq!(
        parsed(b"#no newline at the end").comment(),
        b"no newline at the end\n"
    );
    let p = parsed(b"p ( a b ) q { c } r [ d ]\n");
    assert_eq!(names(&p), ["p", "q", "r"]);
    assert_eq!(names(p.child(0).unwrap()), ["a", "b"]);
}

/// A quoted name holds anything but an unescaped quote or line break (a line break ends it, as
/// does the text's end); a backslash escapes the next character in either form; an unquoted name
/// ends at white space, a comma, a quote, `#` or a bracket. Written back, a name is quoted when
/// it is empty or holds a space, tab, comma, `#` or bracket, and a line break, carriage return,
/// quote or backslash gets a backslash.
///
/// Behaviour: data.text-tree.reads-as-the-client-parses
#[test]
fn quotes_and_escapes_read_and_write_as_the_client_does() {
    let root = parsed(
        b"\"a b\" \"\" \"x\\\"y\" \"back\\\\slash\" \"tab\there\" \"c,d\" \"e#f\" \"g[h]\"\n",
    );
    assert_eq!(
        names(&root),
        [
            "a b",
            "",
            "x\"y",
            "back\\slash",
            "tab\there",
            "c,d",
            "e#f",
            "g[h]"
        ]
    );
    assert_eq!(
        written(b"\"a b\" \"\" \"x\\\"y\" \"back\\\\slash\"\n"),
        "\n\"a b\"\n\n\"\"\n\nx\\\"y\n\nback\\\\slash\n"
    );
    let root = parsed(b"un\\ quoted\\ space plain\\\\ esc\\\"q\n");
    assert_eq!(names(&root), ["un quoted space", "plain\\", "esc\"q"]);
    // A line break ends a quoted name; the quote that was to close it opens the next.
    let root = parsed(b"\"line\\\nbreak\" \"bad\nquote\" after\n");
    assert_eq!(names(&root), ["line\nbreak", "bad", "quote", " after"]);
    assert_eq!(
        names(&parsed(b"a,b,,c\t d\r\ne\n")),
        ["a", "b", "c", "d", "e"]
    );
    assert_eq!(written(b"\"line\\\nbreak\"\n"), "\nline\\\nbreak\n");
    assert_eq!(names(&parsed(b"\"unterminated")), ["unterminated"]);
    assert_eq!(names(&parsed(b"\"escape at end\\")), ["escape at end"]);
    assert_eq!(names(&parsed(b"ends\\")), ["ends"]);
    // High bytes are characters like any other.
    let root = parsed(b"\xe9t\xe9 \x80\x81\xfe\n");
    assert_eq!(
        root.child(0).unwrap().name_text_now().unwrap(),
        b"\xe9t\xe9"
    );
    assert_eq!(
        root.child(1).unwrap().name_text_now().unwrap(),
        b"\x80\x81\xfe"
    );
}

/// A parse fails when a bracket closes that is not open or not the one open, when a bracket is
/// never closed, and when there is nothing at all to read (an empty text, an empty file); white
/// space alone reads as nothing. A failed parse keeps the nodes read before the failure.
///
/// Behaviour: data.text-tree.reads-as-the-client-parses
#[test]
fn a_stray_or_mismatched_bracket_or_an_empty_text_fails_the_parse() {
    let fails = |t: &[u8]| {
        let mut root = Node::new();
        let ok = Parser::new(&tree::NoIncludes).parse_string(&mut root, t);
        (ok, names(&root))
    };
    assert_eq!(fails(b"mismatch [ a )\n"), (false, vec![]));
    assert_eq!(fails(b"stray ] after\n"), (false, vec!["stray".to_owned()]));
    assert_eq!(fails(b"unclosed [ a b\n"), (false, vec![]));
    assert_eq!(fails(b""), (false, vec![]));
    assert_eq!(fails(b"   \n\t\n"), (true, vec![]));
    assert!(tree::parse_bytes(b"empty", b"").is_none());
    assert!(tree::parse_bytes(b"blank", b"\r\n").is_some());
}

/// Finds a file named in the parse by its name.
#[derive(Debug)]
struct Files(Vec<(&'static [u8], &'static [u8])>);

impl Includes for Files {
    fn find(&self, name: &[u8], _from: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.0
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(n, b)| (n.to_vec(), b.to_vec()))
            .collect()
    }
}

/// `!include [ file ]` puts the named file's nodes in its place, the first carrying the
/// directive's comment, the file's leading comment and its own; a file not found, or one that
/// does not parse, fails the parse. `!error` fails it; `!warning` is dropped; any other name
/// starting with `!` is an ordinary node.
///
/// Behaviour: data.text-tree.directives
#[test]
fn directives_include_fail_or_drop_as_the_client_does() {
    let files = Files(vec![
        (b"keys.txt", b"# file head\n\n# own\nk1 [ a ]\nk2\n"),
        (b"broken.txt", b"x ]"),
    ]);
    let mut root = Node::new();
    let ok = Parser::new(&files)
        .parse_string(&mut root, b"before\n# lead\n!include [ keys.txt ]\nafter\n");
    assert!(ok);
    assert_eq!(names(&root), ["before", "k1", "k2", "after"]);
    assert_eq!(
        root.child(1).unwrap().comment(),
        b" lead\n file head\n own\n"
    );
    for bad in [
        &b"!include [ missing.txt ]\n"[..],
        b"!include [ broken.txt ]\n",
    ] {
        let mut root = Node::new();
        assert!(!Parser::new(&files).parse_string(&mut root, bad));
    }
    let mut root = Node::new();
    let ok = Parser::new(&tree::NoIncludes).parse_string(
        &mut root,
        b"!warning [ ignored ] kept !other [ x ] !error [ stop ] after\n",
    );
    assert!(!ok);
    assert_eq!(names(&root), ["kept", "!other", "after"]);
    assert!(tree::parse_text(b"!warning [ w ] next\n").is_some());
}

/// The writer puts a blank line before each of the root's children and writes a node's children
/// on its line, separated by spaces, unless the node is too wide (its name, six for the
/// brackets, and its children's names past 80 characters, measured three levels down) or a child
/// has a comment: then each child takes a line, indented two spaces a level, and a blank line
/// follows the closing bracket before the next node. A comment is written before its node, a
/// `#` before each of its lines.
///
/// Behaviour: data.text-tree.writes-as-the-client-writes
#[test]
fn the_writer_spreads_a_node_too_wide_too_deep_or_with_commented_children() {
    let wide: Vec<String> = (0..20).map(|i| format!("word{i:02}")).collect();
    let text = format!("short [ a b c ]\nwide [ {} ]\n", wide.join(" "));
    let spread: String = wide.iter().map(|w| format!("  {w}\n")).collect();
    assert_eq!(
        written(text.as_bytes()),
        format!("\nshort [ a b c ]\n\nwide\n[\n{spread}]\n")
    );
    assert_eq!(
        written(b"deep [ a [ b [ c [ d ] ] ] ]\nthree [ a [ b [ c ] ] ]\n"),
        "\ndeep\n[\n  a\n  [\n    b [ c [ d ] ]\n  ]\n]\n\nthree\n[\n  a [ b [ c ] ]\n]\n"
    );
    assert_eq!(
        written(b"multi [ a\n# one\n# two\nb ]\n"),
        "\nmulti\n[\n  a\n\n  # one\n  # two\n  b\n]\n"
    );
    assert_eq!(
        written(b"c1 [ a ]\n# lead c2\nc2 [ b ]\nc3\n"),
        "\nc1 [ a ]\n\n# lead c2\nc2 [ b ]\n\nc3\n"
    );
    assert_eq!(
        written(b"# header\nfirst [ a b ]\n"),
        "\n# header\n\nfirst [ a b ]\n"
    );
    let k2: Vec<String> = (0..25).map(|i| format!("w{i:02}")).collect();
    let k2_lines: String = k2.iter().map(|w| format!("    {w}\n")).collect();
    assert_eq!(
        written(format!("kids [ k1 [ a ] k2 [ {} ] k3 ]\n", k2.join(" ")).as_bytes()),
        format!("\nkids\n[\n  k1 [ a ]\n\n  k2\n  [\n{k2_lines}  ]\n\n  k3\n]\n")
    );
}

/// A file is read as the C library reads one in text mode (each carriage return before a line
/// feed dropped, nothing after a Ctrl-Z, a 0xFF byte the end) and written with each line break as
/// a carriage return and a line feed.
///
/// Behaviour: data.text-tree.file-reads-in-text-mode
#[test]
fn a_file_reads_in_text_mode_and_writes_its_line_breaks_as_crlf() {
    assert_eq!(text_mode(b"a\r\nb\rc\r\n\x1Aafter"), b"a\nb\rc\n");
    let mut root =
        tree::parse_bytes(b"Main.keymap", b"# c\r\nVERSION [ 1 ]\r\n\x1Ajunk ]").expect("parses");
    assert_eq!(root.comment(), b" c\n");
    assert_eq!(names(&root), ["VERSION"]);
    assert_eq!(root.name_text_now().as_deref(), Some(&b"Main.keymap"[..]));
    assert_eq!(
        root.file_info().map(|f| f.filename.clone()),
        Some(b"Main.keymap".to_vec())
    );
    assert_eq!(
        tree::to_file_bytes(&mut root),
        b"\r\n# c\r\n\r\nVERSION [ 1 ]\r\n".to_vec()
    );
    // A 0xFF byte ends a file where it stands.
    let root = tree::parse_bytes(b"f", b"a b\xFF c").expect("parses");
    assert_eq!(names(&root), ["a", "b"]);
}

/// A name the client sets as a number or flag is written in its C form (`%lu`, `%ld`, `%.8g` of a
/// float, `%.16g` of a double, `0x%08x`, `0x%016I64X`, `true`/`false`); read as a number, a name
/// is read whole as the C library reads one (base by its prefix, a sign, no trailing text, in
/// range) and becomes that number.
///
/// Behaviour: data.text-tree.typed-names
#[test]
fn a_name_reads_and_writes_as_a_number_in_the_c_librarys_forms() {
    let mut root = Node::new();
    root.make_child_u32(4_000_000_000);
    root.make_child_i32(-5);
    root.make_child_f32(0.3);
    root.make_child().set_name_f64(1e10);
    root.make_child().set_name_hex(0xAB);
    root.make_child()
        .set_name_instance_id(0x1234_5678_9ABC_DEF0);
    root.make_child_bool(true);
    assert_eq!(
        String::from_utf8(tree::to_text(&mut root)).unwrap(),
        "\n4000000000\n\n-5\n\n0.30000001\n\n10000000000\n\n0x000000ab\n\n0x123456789ABCDEF0\n\ntrue\n"
    );
    let mut n = Node::named("0x1F");
    assert_eq!(n.name_u32(), Some(31));
    assert_eq!(n.name_kind(), NameKind::U32);
    assert_eq!(n.name_text().as_deref(), Some(&b"31"[..]));
    assert_eq!(Node::named("12 ").name_u32(), None);
    assert_eq!(Node::named("-1").name_u32(), Some(u32::MAX));
    assert_eq!(Node::named("4294967296").name_u32(), None);
    assert_eq!(Node::named("010").name_i32(), Some(8));
    assert_eq!(Node::named("TRUE").name_bool(), Some(true));
    assert_eq!(Node::named("yes").name_bool(), Some(false));
    assert_eq!(Node::named("1.5e3").name_f32(), Some(1500.0));
    assert!(number::is_uint32(b""), "nothing at all reads whole");
    assert!(!number::is_uint32(b" "));
    assert!(number::is_uint32(b" 12"));
    assert_eq!(
        number::strtoul(b"0x").end,
        0,
        "a prefix with no digit reads nothing"
    );
    assert_eq!(number::strtoul(b"017").value, 15);
    assert!(number::strtol(b"2147483648").out_of_range);
    assert_eq!(number::strtol(b"-2147483648").value, i32::MIN);
    assert_eq!(number::strtoi64(b"-9223372036854775808").value, i64::MIN);
    assert_eq!(number::strtoi64(b"99999999999999999999").value, i64::MAX);
    assert_eq!(number::strtoui64(b"-1").value, u64::MAX);
    let wrapped = number::strtoui64(b"-99999999999999999999");
    assert_eq!((wrapped.value, wrapped.out_of_range), (1, true));
    assert!(number::is_double(b"1d2"), "an exponent may be written d");
    assert!(!number::is_double(b"1e"));
    assert_eq!(number::strtod(b"1e").end, 1);
    assert!(!number::is_double(b"."));
    assert_eq!(number::general(1.5e-5, 8), "1.5e-005");
    assert_eq!(number::general(123_456_789.0, 8), "1.2345679e+008");
    assert_eq!(Name::F64(1e10).to_text(), b"10000000000");
    assert_eq!(Name::F64(1e17).to_text(), b"1e+017");
}

/// Behaviour: data.tree.a-paths-folder-runs-to-its-last-separator
#[test]
fn a_paths_folder_runs_to_its_last_separator() {
    use auberean_dat::tree::directory_of;
    assert_eq!(directory_of(b"a\\b/c.txt"), b"a\\b/");
    assert_eq!(directory_of(b"c:file"), b"c:");
    assert_eq!(directory_of(b"file"), b"");
    assert_eq!(directory_of(b""), b"");
}
