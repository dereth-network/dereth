//! The files scripts write, in the client's own folder through the host's local system.

use std::collections::BTreeMap;

use auberean_client_runtime::files::{file_name, text_lines, timestamp, Files, Open};
use auberean_script::local::{bare_name, LocalSystem, LocalTime};

/// A folder in memory, and a clock stopped at one time.
#[derive(Debug, Default)]
pub struct Folder {
    pub files: BTreeMap<String, Vec<u8>>,
    pub now: Option<LocalTime>,
}

impl LocalSystem for Folder {
    fn local_time(&self) -> Option<LocalTime> {
        self.now
    }

    fn real_time(&self) -> Option<i64> {
        self.now.map(|_| 1_362_492_429)
    }

    fn append_line(&mut self, file: &str, bytes: &[u8]) -> bool {
        self.files
            .entry(file.to_owned())
            .or_default()
            .extend_from_slice(bytes);
        true
    }

    fn write_file(&mut self, file: &str, bytes: &[u8]) -> bool {
        self.files.insert(file.to_owned(), bytes.to_vec());
        true
    }

    fn read_file(&mut self, file: &str) -> Option<Vec<u8>> {
        self.files.get(file).cloned()
    }
}

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn bytes(s: &str) -> Vec<u8> {
    let mut b = vec![0xFF, 0xFE];
    b.extend(s.encode_utf16().flat_map(u16::to_le_bytes));
    b
}

/// A file starts with its byte-order mark; each line feed is written after a carriage return, and
/// a line that does not end in one gets both; writing to a file that is not open opens it to add
/// to it; opening to add keeps what a file holds unless it does not start with the mark; opening
/// to read needs the file; closing answers whether it was open.
///
/// Behaviour: script.natives.files-scripts-write
#[test]
fn a_file_is_marked_and_its_lines_end_in_a_carriage_return_and_a_line_feed() {
    let mut f = Files::default();
    let mut disk = Folder::default();
    let log = units("chat.txt");
    let name = "chat.txt";
    assert!(
        !f.open(&mut disk, &log, name, true, false),
        "nothing to read yet"
    );
    assert!(f.write(&mut disk, &log, name, &units("hello")));
    assert!(f.write(&mut disk, &log, name, &units("a\nb\n")));
    assert_eq!(disk.files[name], bytes("hello\r\na\r\nb\r\n"));
    assert!(f.close(&log));
    assert!(!f.close(&log), "closed already");
    // Adding to it keeps what it holds; writing it anew starts again.
    assert!(f.open(&mut disk, &log, name, false, true));
    assert!(f.write(&mut disk, &log, name, &units("")));
    assert_eq!(disk.files[name], bytes("hello\r\na\r\nb\r\n\r\n"));
    assert!(f.open(&mut disk, &log, name, false, false));
    assert_eq!(disk.files[name], bytes(""));
    // Open to read, it takes nothing written to it.
    assert!(f.open(&mut disk, &log, name, true, false));
    assert_eq!(f.open[&log], Open::Read);
    assert!(f.write(&mut disk, &log, name, &units("x")));
    assert_eq!(disk.files[name], bytes(""));
    // A file that does not start with the mark is not added to.
    let other = units("other.txt");
    disk.files.insert("other.txt".into(), b"ab".to_vec());
    assert!(!f.open(&mut disk, &other, "other.txt", false, true));
    assert!(!f.write(&mut disk, &other, "other.txt", &units("x")));
    assert_eq!(disk.files["other.txt"], b"ab");
    // An empty file gets the mark.
    disk.files.insert("empty.txt".into(), Vec::new());
    assert!(f.open(&mut disk, &units("empty.txt"), "empty.txt", false, true));
    assert_eq!(disk.files["empty.txt"], bytes(""));
}

/// A file reads as narrow text: a carriage return before a line feed goes, a lone one stays; a
/// Ctrl+Z ends the text; the code page's bytes become its characters; a line stops at a zero; a line
/// over 4095 characters goes on as a next one. So a file a script wrote reads as its mark and first
/// letter.
///
/// Behaviour: script.natives.a-file-reads-as-narrow-text
#[test]
fn a_file_reads_as_narrow_text_a_line_at_a_time() {
    let line = |s: &str| units(s);
    assert_eq!(
        text_lines(b"one\r\ntwo\n\nfour\rfive"),
        [line("one"), line("two"), line(""), line("four\rfive")]
    );
    assert_eq!(text_lines(b"seen\n\x1Anot seen\n"), [line("seen")]);
    assert_eq!(text_lines(b"\x80\xE9\n"), [[0x20AC, 0xE9]]);
    assert_eq!(text_lines(b"cut\0off\nnext"), [line("cut"), line("next")]);
    assert_eq!(text_lines(b""), Vec::<Vec<u16>>::new());
    let long = vec![b'a'; 4096];
    let read = text_lines(&long);
    assert_eq!((read[0].len(), read[1].len()), (4095, 1));
    // A file a script wrote: its mark, the first letter, then a zero ends the line; the line
    // feed's high byte, a zero, is a line with nothing on it.
    assert_eq!(
        text_lines(&bytes("hi\r\n")),
        [vec![0xFF, 0xFE, u16::from(b'h')], Vec::new()]
    );
}

/// Reading a file that is not open opens it to read and fails when there is none; reading uses it
/// up, its name staying open until it is closed; a file open to write gives nothing and is used up
/// too, so a line written to it after is taken and dropped.
///
/// Behaviour: script.natives.reading-a-file-uses-it-up
#[test]
fn reading_a_file_opens_it_and_uses_it_up() {
    let mut f = Files::default();
    let mut disk = Folder::default();
    let path = units("notes.txt");
    assert_eq!(f.read(&mut disk, &path, "notes.txt"), None);
    assert!(f.open.is_empty(), "nothing opened");
    disk.files.insert("notes.txt".into(), b"a\nb\n".to_vec());
    assert_eq!(
        f.read(&mut disk, &path, "notes.txt"),
        Some(vec![units("a"), units("b")])
    );
    assert_eq!(f.open.get(&path), Some(&Open::UsedUp));
    assert_eq!(f.read(&mut disk, &path, "notes.txt"), Some(Vec::new()));
    assert!(f.close(&path));
    assert_eq!(
        f.read(&mut disk, &path, "notes.txt"),
        Some(vec![units("a"), units("b")]),
        "closed, it reads again"
    );
    assert!(f.close(&path));
    let out = units("out.txt");
    assert!(f.open(&mut disk, &out, "out.txt", false, false));
    assert_eq!(f.read(&mut disk, &out, "out.txt"), Some(Vec::new()));
    assert!(f.write(&mut disk, &out, "out.txt", &units("gone")));
    assert_eq!(disk.files["out.txt"], [0xFF, 0xFE]);
}

/// A line written with a timestamp starts with the local time as the C library writes
/// `<%b %d %Y, %H:%M:%S>  `; without a clock the line is written alone. A host with no local
/// system opens nothing and writes nothing.
///
/// Behaviour: script.natives.a-log-line-is-stamped-with-the-local-time
#[test]
fn a_log_line_is_stamped_with_the_local_time_and_nothing_opens_without_a_disk() {
    struct Nothing;
    impl LocalSystem for Nothing {}
    let now = LocalTime {
        year: 2013,
        month: 3,
        day: 5,
        weekday: 2,
        hour: 14,
        minute: 7,
        second: 9,
        millisecond: 120,
    };
    assert_eq!(timestamp(&now), "<Mar 05 2013, 14:07:09>  ");
    let mut f = Files::default();
    let mut disk = Folder {
        now: Some(now),
        ..Folder::default()
    };
    let log = units("Bob.log");
    assert!(f.write_stamped(&mut disk, &log, "Bob.log", Some(&now), &units("hi")));
    assert!(f.write_stamped(&mut disk, &log, "Bob.log", None, &units("there")));
    assert_eq!(
        disk.files["Bob.log"],
        bytes("<Mar 05 2013, 14:07:09>  hi\r\nthere\r\n")
    );
    // The default local system refuses every file.
    let mut f = Files::default();
    assert!(!f.open(&mut Nothing, &log, "Bob.log", false, true));
    assert!(!f.open(&mut Nothing, &log, "Bob.log", false, false));
    assert!(!f.write(&mut Nothing, &log, "Bob.log", &units("x")));
    assert!(f.open.is_empty());
}

/// A file is named by a bare name in the client's own folder: a path built on the output path
/// names the file after it, and a name with a folder, a drive, `.`, `..` or nothing is refused,
/// where the original client wrote wherever a path pointed.
///
/// Behaviour: script.natives.files-stay-in-the-settings-folder
#[test]
fn a_file_is_named_by_a_bare_name_and_any_other_is_refused() {
    for good in ["chat.txt", "Bob.log", "a..b", ".hidden", "with space.txt"] {
        assert!(bare_name(good), "{good}");
    }
    for bad in [
        "",
        ".",
        "..",
        "../x",
        "a/b",
        "a\\b",
        "C:x",
        "C:\\log.txt",
        "\\\\server\\x",
        "a\0b",
    ] {
        assert!(!bare_name(bad), "{bad:?}");
    }
    assert_eq!(
        file_name(&[], &units("\\Bob.log")).as_deref(),
        Some("Bob.log")
    );
    assert_eq!(
        file_name(&units("out"), &units("out\\Bob.log")).as_deref(),
        Some("Bob.log")
    );
    assert_eq!(
        file_name(&units("out"), &units("C:\\Bob.log")).as_deref(),
        Some("C:\\Bob.log")
    );
    assert_eq!(file_name(&[], &[0xD800]), None, "not text");
}

/// Through the client's host: a file name that is not bare is refused even by a host that would
/// take it, the file natives open nothing when the host gives no local system, and a stamped line
/// is written to the host's folder with its local time.
///
/// Behaviour: script.natives.the-client-host-keeps-files-in-its-folder
#[test]
fn the_client_host_refuses_names_that_leave_its_folder_and_writes_stamped_lines() {
    use auberean_script::native_object::{native_type, NativeObject};
    use auberean_script::value::tag;
    use auberean_script::Host as _;

    let mut b = crate::common::Bench::new();
    let text = |b: &mut crate::common::Bench, s: &str| {
        b.vm.heap
            .add_native(native_type::WP_STRING, NativeObject::Text(units(s)))
    };
    let (path, line) = (text(&mut b, "\\Bob.log"), text(&mut b, "hi"));
    let args = [(path, tag::REFERENCE), (line, tag::REFERENCE)];
    // No local system: nothing opens, nothing is written.
    assert_eq!(b.answer(0x26, 0x05, 0, &args), 0);
    assert!(!b.host.write_file("Bob.log", b"x"));
    assert_eq!(b.host.read_file("Bob.log"), None);
    assert_eq!((b.host.local_time(), b.host.real_time()), (None, None));
    let now = LocalTime {
        year: 2017,
        month: 1,
        day: 9,
        hour: 23,
        minute: 59,
        second: 1,
        ..LocalTime::default()
    };
    b.host.local = Some(Box::new(Folder {
        now: Some(now),
        ..Folder::default()
    }));
    for bad in [
        "",
        "..",
        "../Bob.log",
        "logs/Bob.log",
        "logs\\Bob.log",
        "C:Bob.log",
    ] {
        assert!(!b.host.write_file(bad, b"x"), "{bad:?}");
        assert!(!b.host.append_line(bad, b"x"), "{bad:?}");
        assert_eq!(b.host.read_file(bad), None, "{bad:?}");
    }
    assert_eq!(b.answer(0x26, 0x05, 0, &args), 1);
    assert_eq!(
        b.host.read_file("Bob.log"),
        Some(bytes("<Jan 09 2017, 23:59:01>  hi\r\n"))
    );
    // A path that leaves the folder does not open.
    let away = text(&mut b, "C:\\logs\\Bob.log");
    assert_eq!(
        b.answer(
            0x26,
            0x05,
            0,
            &[(away, tag::REFERENCE), (line, tag::REFERENCE)]
        ),
        0
    );
}
