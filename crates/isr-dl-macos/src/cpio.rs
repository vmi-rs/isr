//! Streaming odc (`070707`) cpio parser extracting the XNU kernel files.

use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use isr_dl::{ProgressFn, ProgressWriter};

use crate::DownloaderError;

/// Size of an odc header without the file name.
const HEADER_SIZE: usize = 76;

/// Magic bytes of an odc header.
const MAGIC: &[u8] = b"070707";

/// Name of the entry terminating the archive.
const TRAILER: &[u8] = b"TRAILER!!!";

/// File type mask of the `mode` field.
const S_IFMT: u32 = 0o170000;

/// Regular file type of the `mode` field.
const S_IFREG: u32 = 0o100000;

/// Destination paths of the extracted kernel files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelFiles {
    /// Extracted kernel binary.
    pub kernel: PathBuf,

    /// Extracted dSYM DWARF companion.
    pub dsym: PathBuf,
}

/// Parser state.
enum State {
    /// Collecting the fixed-size header.
    Header,

    /// Collecting the NUL-terminated name.
    Name {
        /// Parsed header fields.
        header: Header,
    },

    /// Consuming `remaining` bytes of file data.
    Data {
        /// Open output, if the entry is extracted.
        output: Option<Output>,

        /// Number of data bytes still expected.
        remaining: u64,
    },

    /// Trailer reached, the rest of the stream is ignored.
    Done,
}

/// Header fields relevant to extraction.
#[derive(Clone, Copy)]
struct Header {
    /// File mode including type bits.
    mode: u32,

    /// Size of the name including the terminating NUL.
    name_size: usize,

    /// Size of the file data.
    file_size: u64,
}

/// Output file of an extracted entry.
struct Output {
    /// Index into `targets`.
    target: usize,

    /// Writer over the `.part` file.
    writer: ProgressWriter<BufWriter<File>>,
}

/// Extraction target matched by path suffix.
struct Target {
    /// Path suffix inside the archive, without a leading `/`.
    suffix: String,

    /// Final destination.
    destination: PathBuf,

    /// Set once the file has been extracted.
    done: bool,
}

/// [`Write`] sink parsing an odc cpio stream and extracting the kernel binary
/// and its dSYM DWARF file.
///
/// Only entries whose path ends with
/// `System/Library/Kernels/<kernel_name>` or
/// `System/Library/Kernels/<kernel_name>.dSYM/Contents/Resources/DWARF/<kernel_name>`
/// are written, each through a sibling `.part` file renamed into place once
/// complete. All other entries are skipped without buffering.
pub struct KernelCpioSink {
    /// Parser state.
    state: State,

    /// Partially collected header or name bytes.
    buffer: Vec<u8>,

    /// Kernel binary and dSYM targets.
    targets: [Target; 2],

    /// Optional extraction progress callback.
    progress: Option<ProgressFn>,
}

impl KernelCpioSink {
    /// Creates a sink extracting `kernel_name` into `kernel` and its DWARF
    /// file into `dsym`.
    pub fn new(
        kernel_name: &str,
        kernel: PathBuf,
        dsym: PathBuf,
        progress: Option<ProgressFn>,
    ) -> Self {
        let kernels = "System/Library/Kernels";

        Self {
            state: State::Header,
            buffer: Vec::with_capacity(HEADER_SIZE),
            targets: [
                Target {
                    suffix: format!("{kernels}/{kernel_name}"),
                    destination: kernel,
                    done: false,
                },
                Target {
                    suffix: format!(
                        "{kernels}/{kernel_name}.dSYM/Contents/Resources/DWARF/{kernel_name}"
                    ),
                    destination: dsym,
                    done: false,
                },
            ],
            progress,
        }
    }

    /// Verifies the archive ended and both files were extracted.
    pub fn finish(self) -> Result<KernelFiles, DownloaderError> {
        if !matches!(self.state, State::Done) {
            return Err(DownloaderError::InvalidCpio("missing trailer"));
        }

        let [kernel, dsym] = self.targets;
        if !kernel.done || !dsym.done {
            let kernel_name = kernel
                .suffix
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_owned();
            return Err(DownloaderError::KernelFilesNotFound(kernel_name));
        }

        Ok(KernelFiles {
            kernel: kernel.destination,
            dsym: dsym.destination,
        })
    }

    /// Consumes bytes from `buf`, returning how many were used.
    fn step(&mut self, buf: &[u8]) -> Result<usize, DownloaderError> {
        match &mut self.state {
            State::Header => {
                let used = fill(&mut self.buffer, buf, HEADER_SIZE);
                if self.buffer.len() == HEADER_SIZE {
                    let header = parse_header(&self.buffer)?;
                    self.buffer.clear();
                    self.state = State::Name { header };
                }
                Ok(used)
            }
            State::Name { header } => {
                let header = *header;
                let used = fill(&mut self.buffer, buf, header.name_size);
                if self.buffer.len() == header.name_size {
                    self.begin_entry(header)?;
                }
                Ok(used)
            }
            State::Data { output, remaining } => {
                let used = usize::try_from(*remaining)
                    .unwrap_or(usize::MAX)
                    .min(buf.len());
                if let Some(output) = output {
                    output.writer.write_all(&buf[..used])?;
                }
                *remaining -= used as u64;
                if *remaining == 0 {
                    self.end_entry()?;
                }
                Ok(used)
            }
            State::Done => Ok(buf.len()),
        }
    }

    /// Handles a fully read name, opening an output if the entry matches.
    fn begin_entry(&mut self, header: Header) -> Result<(), DownloaderError> {
        let name = match self.buffer.split_last() {
            Some((0, name)) => name,
            _ => return Err(DownloaderError::InvalidCpio("name not NUL-terminated")),
        };

        if name == TRAILER {
            self.buffer.clear();
            self.state = State::Done;
            return Ok(());
        }

        let target = if header.mode & S_IFMT == S_IFREG {
            self.targets
                .iter()
                .position(|target| !target.done && matches_suffix(name, &target.suffix))
        }
        else {
            None
        };
        self.buffer.clear();

        let output = match target {
            Some(target) => {
                let destination = &self.targets[target].destination;
                tracing::debug!(path = %destination.display(), "extracting");
                if let Some(parent) = destination.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let file = File::create(part_path(destination))?;
                Some(Output {
                    target,
                    writer: ProgressWriter::for_extract(
                        self.progress.clone(),
                        BufWriter::new(file),
                        destination,
                        Some(header.file_size),
                    ),
                })
            }
            None => None,
        };

        self.state = State::Data {
            output,
            remaining: header.file_size,
        };

        if header.file_size == 0 {
            self.end_entry()?;
        }

        Ok(())
    }

    /// Finalizes the current entry and renames its `.part` file into place.
    fn end_entry(&mut self) -> Result<(), DownloaderError> {
        let state = std::mem::replace(&mut self.state, State::Header);

        if let State::Data {
            output: Some(mut output),
            ..
        } = state
        {
            output.writer.flush()?;
            drop(output.writer);

            let target = &mut self.targets[output.target];
            std::fs::rename(part_path(&target.destination), &target.destination)?;
            target.done = true;
        }

        Ok(())
    }
}

impl Write for KernelCpioSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut offset = 0;

        while offset < buf.len() {
            offset += self.step(&buf[offset..]).map_err(std::io::Error::other)?;
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Appends bytes from `buf` to `buffer` until it holds `target` bytes.
fn fill(buffer: &mut Vec<u8>, buf: &[u8], target: usize) -> usize {
    let used = (target - buffer.len()).min(buf.len());
    buffer.extend_from_slice(&buf[..used]);
    used
}

/// Parses the fixed-size odc header.
fn parse_header(header: &[u8]) -> Result<Header, DownloaderError> {
    if &header[..6] != MAGIC {
        return Err(DownloaderError::InvalidCpio("bad header magic"));
    }

    let mode = octal(&header[18..24])?;
    let name_size = octal(&header[59..65])?;
    let file_size = octal(&header[65..76])?;

    if name_size == 0 {
        return Err(DownloaderError::InvalidCpio("empty name"));
    }

    Ok(Header {
        mode: mode as u32,
        name_size: name_size as usize,
        file_size,
    })
}

/// Parses an ASCII octal field.
fn octal(field: &[u8]) -> Result<u64, DownloaderError> {
    field.iter().try_fold(0u64, |value, digit| match digit {
        b'0'..=b'7' => Ok((value << 3) | u64::from(digit - b'0')),
        _ => Err(DownloaderError::InvalidCpio("bad octal field")),
    })
}

/// Returns whether `name` ends with `/` + `suffix` or equals `suffix`
/// (ignoring a leading `./`).
fn matches_suffix(name: &[u8], suffix: &str) -> bool {
    let name = name.strip_prefix(b"./").unwrap_or(name);
    let suffix = suffix.as_bytes();

    match name.strip_suffix(suffix) {
        Some(prefix) => prefix.is_empty() || prefix.ends_with(b"/"),
        None => false,
    }
}

/// Returns the sibling `.part` path of `path`.
fn part_path(path: &Path) -> PathBuf {
    path.with_added_extension("part")
}

#[cfg(test)]
mod tests {
    use super::*;

    const KERNEL: &str = "kernel.release.vmapple";

    fn entry(archive: &mut Vec<u8>, name: &str, mode: u32, data: &[u8]) {
        let header = format!(
            "070707{:06o}{:06o}{:06o}{:06o}{:06o}{:06o}{:06o}{:011o}{:06o}{:011o}",
            0,
            1,
            mode,
            0,
            0,
            1,
            0,
            0,
            name.len() + 1,
            data.len()
        );
        archive.extend_from_slice(header.as_bytes());
        archive.extend_from_slice(name.as_bytes());
        archive.push(0);
        archive.extend_from_slice(data);
    }

    fn archive() -> Vec<u8> {
        let kernels = "./Library/Developer/KDKs/KDK.kdk/System/Library/Kernels";
        let mut archive = Vec::new();
        entry(&mut archive, ".", 0o040755, b"");
        entry(&mut archive, "./empty", 0o100644, b"");
        entry(
            &mut archive,
            &format!("{kernels}/kernel.release.t8103"),
            0o100644,
            b"wrong kernel",
        );
        entry(
            &mut archive,
            &format!("{kernels}/x{KERNEL}"),
            0o100644,
            b"wrong suffix",
        );
        entry(
            &mut archive,
            &format!("{kernels}/{KERNEL}"),
            0o100644,
            b"kernel bytes",
        );
        entry(
            &mut archive,
            &format!("{kernels}/{KERNEL}.dSYM/Contents/Resources/DWARF/{KERNEL}"),
            0o100644,
            &[0xAB; 1000],
        );
        entry(&mut archive, "TRAILER!!!", 0, b"");
        archive.extend_from_slice(&[0; 300]);
        archive
    }

    fn sink(dir: &Path) -> KernelCpioSink {
        KernelCpioSink::new(
            KERNEL,
            dir.join(KERNEL),
            dir.join("dsym").join(KERNEL),
            None,
        )
    }

    #[test]
    fn extracts_matching_entries_across_write_boundaries() {
        let archive = archive();

        for chunk_size in [1, 7, 76, 77, 4096] {
            let dir = tempfile::tempdir().unwrap();
            let mut sink = sink(dir.path());

            for chunk in archive.chunks(chunk_size) {
                sink.write_all(chunk).unwrap();
            }

            let files = sink.finish().unwrap();
            assert_eq!(std::fs::read(&files.kernel).unwrap(), b"kernel bytes");
            assert_eq!(std::fs::read(&files.dsym).unwrap(), vec![0xAB; 1000]);

            let mut names = std::fs::read_dir(dir.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect::<Vec<_>>();
            names.sort();
            assert_eq!(names, ["dsym", KERNEL]);
        }
    }

    #[test]
    fn extracts_zero_size_target() {
        let dir = tempfile::tempdir().unwrap();
        let mut archive = Vec::new();
        entry(
            &mut archive,
            &format!("System/Library/Kernels/{KERNEL}"),
            0o100644,
            b"",
        );
        entry(
            &mut archive,
            &format!("System/Library/Kernels/{KERNEL}.dSYM/Contents/Resources/DWARF/{KERNEL}"),
            0o100644,
            b"",
        );
        entry(&mut archive, "TRAILER!!!", 0, b"");

        let mut sink = sink(dir.path());
        sink.write_all(&archive).unwrap();

        let files = sink.finish().unwrap();
        assert_eq!(std::fs::read(&files.kernel).unwrap(), b"");
        assert_eq!(std::fs::read(&files.dsym).unwrap(), b"");
    }

    #[test]
    fn missing_dsym_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let mut archive = Vec::new();
        entry(
            &mut archive,
            &format!("System/Library/Kernels/{KERNEL}"),
            0o100644,
            b"k",
        );
        entry(&mut archive, "TRAILER!!!", 0, b"");

        let mut sink = sink(dir.path());
        sink.write_all(&archive).unwrap();

        assert!(matches!(
            sink.finish(),
            Err(DownloaderError::KernelFilesNotFound(name)) if name == KERNEL
        ));
    }

    #[test]
    fn missing_trailer_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let mut archive = archive();
        let trailer = archive.len() - 300 - HEADER_SIZE - TRAILER.len() - 1;
        archive.truncate(trailer);

        let mut sink = sink(dir.path());
        sink.write_all(&archive).unwrap();

        assert!(matches!(
            sink.finish(),
            Err(DownloaderError::InvalidCpio(_))
        ));
    }

    #[test]
    fn rejects_bad_magic() {
        let dir = tempfile::tempdir().unwrap();
        let mut archive = archive();
        archive[0] = b'1';

        let mut sink = sink(dir.path());
        assert!(sink.write_all(&archive).is_err());
    }
}
