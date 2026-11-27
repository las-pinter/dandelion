//! Paging and tail behaviour on real files.

use std::io::Write;

use super::tail::TAIL_READ;
use super::{Cursor, page_before, tail};
use crate::transcript::records;

fn fixture(name: &str, lines: &[&str]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(name);
    let mut file = std::fs::File::create(&path).expect("file");
    for line in lines {
        writeln!(file, "{line}").expect("line");
    }
    (dir, path)
}

fn user(text: &str) -> String {
    format!(r#"{{"type":"user","message":{{"content":"{text}"}}}}"#)
}

#[test]
fn a_page_holds_the_last_records_and_knows_more_precede_it() {
    let one = user("one");
    let two = user("two");
    let lines = [
        one.as_str(),
        r#"{"type":"cost-state","totalCostUSD":1.5}"#,
        two.as_str(),
        "",
    ];
    let (_dir, path) = fixture("transcript.jsonl", &lines);

    let page = page_before(&path, None, 1).expect("page");

    assert_eq!(page.records.len(), 1);
    let text = serde_json::to_string(&page.records[0]).expect("json");
    assert!(text.contains("two"), "{text}");
    assert!(page.has_earlier);
    assert!(page.start > 0);
}

#[test]
fn an_earlier_page_ends_where_the_previous_one_started() {
    let one = user("one");
    let two = user("two");
    let three = user("three");
    let lines = [one.as_str(), two.as_str(), three.as_str()];
    let (_dir, path) = fixture("transcript.jsonl", &lines);

    let last = page_before(&path, None, 1).expect("page");
    let earlier = page_before(&path, Some(last.start), 1).expect("page");

    let text = serde_json::to_string(&earlier.records).expect("json");
    assert!(text.contains("two"), "{text}");
    assert!(!text.contains("three"), "{text}");
    assert!(earlier.has_earlier);

    let oldest = page_before(&path, Some(earlier.start), 1).expect("page");
    let text = serde_json::to_string(&oldest.records).expect("json");
    assert!(text.contains("one"), "{text}");
    assert_eq!(oldest.start, 0);
    assert!(!oldest.has_earlier);
}

#[test]
fn a_page_at_the_start_reports_nothing_earlier() {
    let only = user("only");
    let (_dir, path) = fixture("transcript.jsonl", &[only.as_str()]);

    let page = page_before(&path, None, 50).expect("page");

    assert_eq!(page.records.len(), 1);
    assert!(!page.has_earlier);
    assert_eq!(page.start, 0);
}

#[test]
fn records_carry_their_byte_offsets() {
    let one = user("one");
    let two = user("two");
    let (_dir, path) = fixture("transcript.jsonl", &[one.as_str(), two.as_str()]);

    let page = page_before(&path, None, 50).expect("page");

    assert_eq!(page.records[0].offset, 0);
    assert_eq!(page.records[1].offset, (one.len() + 1) as u64);
}

#[test]
fn a_page_crosses_a_block_without_duplicating_records() {
    let big = user(&"x".repeat(70 * 1024));
    let one = user("one");
    let two = user("two");
    let (_dir, path) = fixture(
        "transcript.jsonl",
        &[big.as_str(), one.as_str(), two.as_str()],
    );

    let page = page_before(&path, None, 50).expect("page");

    let offsets: Vec<u64> = page.records.iter().map(|record| record.offset).collect();
    assert_eq!(offsets.len(), 3, "each record appears once");
    assert!(
        offsets.windows(2).all(|pair| pair[0] < pair[1]),
        "{offsets:?}"
    );
    assert_eq!(offsets[0], 0);
    assert!(!page.has_earlier);
}

#[test]
fn a_record_longer_than_a_block_does_not_stall_the_page() {
    let one = user("one");
    let big = user(&"y".repeat(70 * 1024));
    let (_dir, path) = fixture("transcript.jsonl", &[one.as_str(), big.as_str()]);

    let page = page_before(&path, None, 50).expect("page");

    assert_eq!(page.records.len(), 2);
    assert_eq!(page.records[0].offset, 0);
    assert_eq!(page.records[1].offset, (one.len() + 1) as u64);
}

#[test]
fn a_page_ends_at_its_last_complete_line() {
    let complete = user("one");
    let partial = user("two");
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("transcript.jsonl");
    std::fs::write(&path, format!("{complete}\n{partial}")).expect("write");

    let page = page_before(&path, None, 50).expect("page");

    assert_eq!(page.records.len(), 1, "the partial line is not a record");
    assert_eq!(
        page.end,
        (complete.len() + 1) as u64,
        "the tail starts after the last newline, so nothing is repeated"
    );
}

#[test]
fn a_longer_replacement_restarts_the_tail() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("transcript.jsonl");
    let first = user("one");
    std::fs::write(&path, format!("{first}\n")).expect("write");
    let mut cursor = Cursor::at(&path, 0).expect("cursor");
    tail(&path, &mut cursor).expect("tail");

    let replacement = user("a replacement line long enough to cross the cursor");
    std::fs::write(&path, format!("{replacement}\n")).expect("replace");

    let found = tail(&path, &mut cursor).expect("tail");
    assert!(found.reset, "a cursor mid-line means the file was replaced");
    assert!(found.records.is_empty(), "a reset sends no records");
    let page = page_before(&path, None, 50).expect("page");
    assert!(
        serde_json::to_string(&page.records)
            .expect("json")
            .contains("replacement")
    );
}

#[test]
fn a_longer_replacement_ending_on_a_newline_still_resets() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("transcript.jsonl");
    let first = user("one");
    let second = user("two");
    let replacement = format!("{first}\n{second}\n");
    let old = first.len() + 1;
    let filler = format!("{:width$}\n", "x", width = old - 1);
    assert_eq!(filler.len(), old, "the old file has the same length");

    std::fs::write(&path, &filler).expect("write");
    let mut cursor = Cursor::at(&path, old as u64).expect("cursor");
    std::fs::write(&path, &replacement).expect("replace");

    let found = tail(&path, &mut cursor).expect("tail");
    assert!(
        found.reset,
        "the bytes before the cursor changed, though the newline matches"
    );
    assert_eq!(found.records.len(), 0, "a reset sends no records");
}

#[test]
fn a_line_over_the_parse_bound_does_not_hide_older_records() {
    let one = user("one");
    let big = user(&"z".repeat(records::MAX_LINE_BYTES + 1024));
    let two = user("two");
    let (_dir, path) = fixture(
        "transcript.jsonl",
        &[one.as_str(), big.as_str(), two.as_str()],
    );

    let page = page_before(&path, None, 50).expect("page");

    assert_eq!(page.records.len(), 2, "the oversized record is skipped");
    assert_eq!(page.records[0].offset, 0);
    assert_eq!(page.records[1].offset, (one.len() + big.len() + 2) as u64);
    assert!(!page.has_earlier);
}

#[test]
fn a_tail_skips_a_line_over_its_window() {
    let (_dir, path) = fixture("transcript.jsonl", &[]);
    let big = user(&"w".repeat(usize::try_from(TAIL_READ).expect("bound") + 1024));
    let after = user("after");
    std::fs::write(&path, format!("{big}\n{after}\n")).expect("write");
    let mut cursor = Cursor::at(&path, 0).expect("cursor");

    let first = tail(&path, &mut cursor).expect("tail");
    assert_eq!(first.records.len(), 0, "the oversized line is skipped");

    let second = tail(&path, &mut cursor).expect("tail");
    let text = serde_json::to_string(&second.records).expect("json");
    assert!(text.contains("after"), "{text}");
}

#[test]
fn the_tail_waits_for_a_complete_line_and_resets_when_the_file_shrinks() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("transcript.jsonl");
    {
        let mut file = std::fs::File::create(&path).expect("file");
        write!(file, "{}", user("first")).expect("write");
    }
    let mut cursor = Cursor::at(&path, 0).expect("cursor");
    assert!(
        tail(&path, &mut cursor).expect("tail").records.is_empty(),
        "no newline yet"
    );

    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("append");
        writeln!(file, "\n{}", user("second")).expect("finish the line");
    }
    let found = tail(&path, &mut cursor).expect("tail");
    assert_eq!(found.records.len(), 2);
    assert!(!found.reset);

    std::fs::write(&path, format!("{}\n", user("fresh"))).expect("replace");
    let found = tail(&path, &mut cursor).expect("tail");
    assert!(found.records.is_empty(), "a reset sends no records");
    assert!(found.reset, "the shrink is reported so the pane reloads");
}
