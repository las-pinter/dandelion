//! Reads transcript pages from a JSONL file by byte offset.
//!
//! A page walks backwards from its end in fixed-size blocks, so a long
//! transcript costs one block of memory plus the page's records, not the file
//! size. A live tail keeps its own offset and never consumes a partial
//! trailing line.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use super::records;

/// Records in one page.
pub const PAGE: usize = 50;
/// Bytes read per backwards block.
const BLOCK: u64 = 64 * 1024;

/// One page of records plus the cursors around it.
#[derive(Debug)]
pub struct Page {
    /// The records, oldest first.
    pub records: Vec<records::Record>,
    /// Byte offset of the first record in the page.
    pub start: u64,
    /// Byte offset just past the last complete line read; the live tail
    /// starts here so no record in the page is repeated or skipped.
    pub end: u64,
    /// Whether a readable record exists before `start`.
    pub has_earlier: bool,
}

/// Reads the last `limit` records that start before `before`, or the end.
///
/// A cursor past the end (the file shrank) is treated as the end.
///
/// # Errors
///
/// Returns the file's error when it cannot be read.
pub fn page_before(path: &Path, before: Option<u64>, limit: usize) -> std::io::Result<Page> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let end = match before {
        Some(before) => before.min(length),
        None => last_line_end(&mut file, length)?,
    };
    if end == 0 {
        return Ok(Page {
            records: Vec::new(),
            start: 0,
            end: 0,
            has_earlier: false,
        });
    }

    let mut entries: Vec<records::Record> = Vec::with_capacity(limit + 1);
    let mut dropped = false;
    // `read_end` is where the next block ends, and it strictly shrinks; the
    // loop therefore terminates. `prefix` holds the leading bytes of a line
    // whose start lies before the block, so the previous block can finish it.
    let mut read_end = end;
    let mut prefix: Vec<u8> = Vec::new();
    loop {
        let block = BLOCK.min(read_end);
        let chunk_start = read_end - block;
        let mut chunk = vec![0_u8; usize::try_from(block).unwrap_or(0)];
        file.seek(SeekFrom::Start(chunk_start))?;
        file.read_exact(&mut chunk)?;
        let mut data = chunk;
        data.extend_from_slice(&prefix);

        let Some(first_newline) = data.iter().position(|byte| *byte == b'\n') else {
            // No complete line yet; carry everything into the next block.
            prefix = data;
            read_end = chunk_start;
            if chunk_start == 0 {
                break;
            }
            continue;
        };
        // A line is complete only when its start is in the buffer: either the
        // block begins the file, or the byte before it ends a line.
        let aligned = chunk_start == 0 || byte_before(&mut file, chunk_start)? == b'\n';
        let from = if aligned { 0 } else { first_newline + 1 };
        let mut block = parse_block(&data[from..], chunk_start + from as u64);
        entries.splice(0..0, block.drain(..));
        if entries.len() > limit {
            let extra = entries.len() - limit;
            entries.drain(0..extra);
            dropped = true;
        }

        prefix = data[..from].to_vec();
        read_end = chunk_start;

        if dropped || chunk_start == 0 {
            break;
        }
    }

    let start = entries.first().map_or(end, |record| record.offset);
    Ok(Page {
        records: entries,
        start,
        end,
        has_earlier: dropped,
    })
}

/// Parses the complete lines of one region, oldest first.
fn parse_block(region: &[u8], base: u64) -> Vec<records::Record> {
    let mut found = Vec::new();
    let mut offset = base;
    for line in region.split_inclusive(|byte| *byte == b'\n') {
        if line.last() != Some(&b'\n') {
            break;
        }
        if let Ok(text) = std::str::from_utf8(line)
            && let Some(mut record) = records::parse_line(text.trim_end_matches(['\n', '\r']))
        {
            record.offset = offset;
            found.push(record);
        }
        offset += line.len() as u64;
    }
    found
}

/// The byte just before `offset`, or a newline when there is none.
fn byte_before(file: &mut File, offset: u64) -> std::io::Result<u8> {
    if offset == 0 {
        return Ok(b'\n');
    }
    let mut byte = [0_u8; 1];
    file.seek(SeekFrom::Start(offset - 1))?;
    file.read_exact(&mut byte)?;
    Ok(byte[0])
}

/// The offset just past the last complete line of the file.
fn last_line_end(file: &mut File, length: u64) -> std::io::Result<u64> {
    let mut end = length;
    while end > 0 {
        let block = BLOCK.min(end);
        let start = end - block;
        let mut chunk = vec![0_u8; usize::try_from(block).unwrap_or(0)];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut chunk)?;
        if let Some(index) = chunk.iter().rposition(|byte| *byte == b'\n') {
            return Ok(start + index as u64 + 1);
        }
        end = start;
    }
    Ok(0)
}

/// What a tail pass found, and whether the file was replaced under it.
#[derive(Debug)]
pub struct Tail {
    /// New records, oldest first.
    pub records: Vec<records::Record>,
    /// Whether the file was replaced, so the caller should reload a page.
    pub reset: bool,
}

/// How many bytes before the cursor identify the file's content there.
const PRINT_BYTES: u64 = 64;

/// A live tail's position: the byte offset plus a fingerprint of the bytes
/// just before it.
///
/// The fingerprint is what catches a replaced file that happens to be longer
/// and to have a newline at the old offset; the offset alone cannot.
#[derive(Debug, Clone, Copy, Default)]
pub struct Cursor {
    offset: u64,
    print: u64,
}

impl Cursor {
    /// A cursor at `offset`, fingerprinted against the file as it is now.
    ///
    /// An offset past the end starts over at zero.
    ///
    /// # Errors
    ///
    /// Returns the file's error when it cannot be read.
    pub fn at(path: &Path, offset: u64) -> std::io::Result<Self> {
        let mut file = File::open(path)?;
        if offset > file.metadata()?.len() {
            return Ok(Self::default());
        }
        Ok(Self {
            offset,
            print: print_at(&mut file, offset)?,
        })
    }
}

/// A hash of the bytes just before `offset`; zero at the file start.
fn print_at(file: &mut File, offset: u64) -> std::io::Result<u64> {
    if offset == 0 {
        return Ok(0);
    }
    let start = offset.saturating_sub(PRINT_BYTES);
    let mut bytes = vec![0_u8; usize::try_from(offset - start).unwrap_or(0)];
    file.seek(SeekFrom::Start(start))?;
    file.read_exact(&mut bytes)?;
    Ok(bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    }))
}

/// Reads complete records written since `cursor` and advances it.
///
/// A trailing line without a newline is left for the next call, so a write in
/// progress is never half-parsed. A replaced file — shrunk, or with different
/// bytes at the cursor — reports `reset` with no records, and the caller
/// reloads a page.
///
/// # Errors
///
/// Returns the file's error when it cannot be read.
pub fn tail(path: &Path, cursor: &mut Cursor) -> std::io::Result<Tail> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let moved = length < cursor.offset
        || (cursor.offset > 0
            && (byte_before(&mut file, cursor.offset)? != b'\n'
                || print_at(&mut file, cursor.offset)? != cursor.print));
    if moved {
        *cursor = Cursor::default();
        return Ok(Tail {
            records: Vec::new(),
            reset: true,
        });
    }
    file.seek(SeekFrom::Start(cursor.offset))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let Some(last_newline) = buffer.iter().rposition(|byte| *byte == b'\n') else {
        return Ok(Tail {
            records: Vec::new(),
            reset: false,
        });
    };
    let complete = &buffer[..=last_newline];
    let mut found = Vec::new();
    let mut line_offset = cursor.offset;
    for line in complete.split(|byte| *byte == b'\n') {
        let Ok(text) = std::str::from_utf8(line) else {
            line_offset += line.len() as u64 + 1;
            continue;
        };
        if let Some(mut record) = records::parse_line(text.trim_end_matches('\r')) {
            record.offset = line_offset;
            found.push(record);
        }
        line_offset += line.len() as u64 + 1;
    }
    cursor.offset += complete.len() as u64;
    cursor.print = print_at(&mut file, cursor.offset)?;
    Ok(Tail {
        records: found,
        reset: false,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::{Cursor, page_before, tail};

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
}
