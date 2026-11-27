//! Reads transcript pages from a JSONL file by byte offset.
//!
//! A page walks backwards from its end in fixed-size blocks, so a long
//! transcript costs one block of memory plus the page's records, not the file
//! size; a line over the parse bound stops the scan. A live tail keeps its own
//! offset and never consumes a partial trailing line, and one pass reads at
//! most a few megabytes, so a burst larger than that waits for the next one.
//!
//! Paging and the live tail sit in their own modules; this one re-exports the
//! public surface and holds the helper both need.

mod page;
mod tail;

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

pub use page::{Page, page_before};
pub use tail::{Cursor, Tail, tail};

/// Records in one page.
pub const PAGE: usize = 50;

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

#[cfg(test)]
mod tests;
