//! The live tail: records written since a byte cursor.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use super::byte_before;
use crate::transcript::records;

/// Most bytes one tail pass reads; a larger burst waits for the next pass.
pub(super) const TAIL_READ: u64 = 4 * 1024 * 1024;

/// How many bytes before the cursor identify the file's content there.
const PRINT_BYTES: u64 = 64;

/// What a tail pass found, and whether the file was replaced under it.
#[derive(Debug)]
pub struct Tail {
    /// New records, oldest first.
    pub records: Vec<records::Record>,
    /// Whether the file was replaced, so the caller should reload a page.
    pub reset: bool,
}

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
    (&mut file).take(TAIL_READ).read_to_end(&mut buffer)?;

    let Some(last_newline) = buffer.iter().rposition(|byte| *byte == b'\n') else {
        if buffer.len() as u64 == TAIL_READ
            && let Some(skipped) = skip_line(&mut file)?
        {
            // One line filled the window; consume it so the tail advances.
            cursor.offset += TAIL_READ + skipped;
            cursor.print = print_at(&mut file, cursor.offset)?;
        }
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

/// Consumes one line longer than the read window, returning the bytes
/// consumed (the newline included), or `None` at the end of the file.
///
/// Memory stays at one chunk; only the scan is proportional to the line.
fn skip_line(file: &mut File) -> std::io::Result<Option<u64>> {
    const CHUNK: usize = 64 * 1024;
    let mut skipped = 0_u64;
    let mut chunk = vec![0_u8; CHUNK];
    loop {
        let read = file.read(&mut chunk)?;
        if read == 0 {
            return Ok(None);
        }
        if let Some(position) = chunk[..read].iter().position(|byte| *byte == b'\n') {
            return Ok(Some(skipped + position as u64 + 1));
        }
        skipped += read as u64;
    }
}
