//! Common download infrastructure for ISR.

mod error;
mod progress;

use std::{
    fs::File,
    io::{BufWriter, Read, Write},
    path::Path,
};

use url::Url;

pub use self::{
    error::Error,
    progress::{ProgressContext, ProgressEvent, ProgressFn, ProgressWriter},
};

/// Streams `reader` into a new file at `dest`, emitting download progress.
///
/// Writes to a sibling `.part` file and renames into place on success so a
/// killed or failed run never leaves a half-written `dest` behind. The caller
/// is responsible for opening the HTTP response.
pub fn stream_download(
    reader: &mut impl Read,
    dest: &Path,
    url: &Url,
    total_bytes: Option<u64>,
    progress: Option<ProgressFn>,
) -> Result<u64, std::io::Error> {
    with_part_file(dest, |file| {
        let writer = BufWriter::new(file);
        let mut writer = ProgressWriter::for_download(progress, writer, url, total_bytes);
        let n = std::io::copy(reader, &mut writer)?;
        writer.flush()?;
        Ok(n)
    })
}

/// Streams `reader` into a new file at `dest`, emitting extraction progress.
///
/// Writes to a sibling `.part` file and renames into place on success so a
/// killed or failed run never leaves a half-extracted `dest` behind.
pub fn stream_extract(
    reader: &mut impl Read,
    dest: &Path,
    total_bytes: Option<u64>,
    progress: Option<ProgressFn>,
) -> Result<u64, std::io::Error> {
    with_part_file(dest, |file| {
        let writer = BufWriter::new(file);
        let mut writer = ProgressWriter::for_extract(progress, writer, dest, total_bytes);
        let n = std::io::copy(reader, &mut writer)?;
        writer.flush()?;
        Ok(n)
    })
}

/// Runs `f` against a sibling `.part` file, then renames it over `dest`.
fn with_part_file<F>(dest: &Path, f: F) -> Result<u64, std::io::Error>
where
    F: FnOnce(File) -> Result<u64, std::io::Error>,
{
    let tmp = dest.with_added_extension("part");
    let file = File::create(&tmp)?;
    let n = f(file)?;
    std::fs::rename(&tmp, dest)?;
    Ok(n)
}
