//! Paging backwards from a byte offset.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use super::byte_before;
use crate::transcript::records;

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
            if prefix.len() > records::MAX_LINE_BYTES {
                // The line is over the parse bound; stop carrying it and keep
                // scanning for the line before it.
                prefix.clear();
            }
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
        if prefix.len() > records::MAX_LINE_BYTES {
            // A line over the parse bound is skipped like any unreadable one;
            // its bytes are dropped rather than carried back to its start.
            prefix.clear();
        }
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
