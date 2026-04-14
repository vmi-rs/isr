use std::{
    io::{Error, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use url::Url;

/// Progress event emitted during download and extraction operations.
pub enum ProgressEvent<'a> {
    /// An HTTP download has started.
    DownloadStarted {
        /// URL being downloaded.
        url: &'a Url,

        /// Total size in bytes, if known from the `Content-Length` header.
        total_bytes: Option<u64>,
    },

    /// Bytes have been received.
    DownloadProgress {
        /// URL being downloaded.
        url: &'a Url,

        /// Number of bytes received so far.
        bytes: u64,

        /// Total size in bytes, if known from the `Content-Length` header.
        total_bytes: Option<u64>,
    },

    /// An HTTP download has completed.
    DownloadComplete {
        /// URL that was downloaded.
        url: &'a Url,
    },

    /// Extraction of a file from an archive has started.
    ExtractStarted {
        /// Path of the file being extracted.
        path: &'a Path,

        /// Total uncompressed size in bytes, if known.
        total_bytes: Option<u64>,
    },

    /// Bytes have been extracted from an archive.
    ExtractProgress {
        /// Path of the file being extracted.
        path: &'a Path,

        /// Number of bytes extracted so far.
        bytes: u64,

        /// Total uncompressed size in bytes, if known.
        total_bytes: Option<u64>,
    },

    /// Extraction of a file from an archive has completed.
    ExtractComplete {
        /// Path of the file that was extracted.
        path: &'a Path,
    },
}

/// Shared, cloneable progress callback.
pub type ProgressFn = Arc<dyn Fn(ProgressEvent<'_>) + Send + Sync>;

/// Distinguishes download vs extraction context for [`ProgressWriter`].
pub enum ProgressContext {
    /// HTTP download context.
    Download {
        /// URL being downloaded.
        url: Url,
    },

    /// Archive extraction context.
    Extract {
        /// Path of the file being extracted.
        path: PathBuf,
    },
}

/// A [`Write`](std::io::Write) adapter that optionally reports progress.
///
/// When `progress` is `None`, the writer is a transparent passthrough.
/// When `progress` is `Some`:
/// - Construction emits `DownloadStarted` or `ExtractStarted`.
/// - Each `write()` emits `DownloadProgress` or `ExtractProgress`.
/// - `Drop` emits `DownloadComplete` or `ExtractComplete`.
pub struct ProgressWriter<W> {
    /// Underlying writer receiving the bytes.
    inner: W,

    /// Optional progress callback; `None` disables reporting.
    progress: Option<ProgressFn>,

    /// Download vs extraction discriminator, carries the URL/path.
    context: ProgressContext,

    /// Running total of bytes written.
    written: u64,

    /// Expected total size, if known.
    total_bytes: Option<u64>,
}

impl<W> ProgressWriter<W> {
    /// Creates a writer for an HTTP download.
    pub fn for_download(
        progress: Option<ProgressFn>,
        inner: W,
        url: &Url,
        total_bytes: Option<u64>,
    ) -> Self {
        let url = url.clone();

        if let Some(progress) = &progress {
            progress(ProgressEvent::DownloadStarted {
                url: &url,
                total_bytes,
            });
        }

        Self {
            inner,
            progress,
            context: ProgressContext::Download { url },
            written: 0,
            total_bytes,
        }
    }

    /// Creates a writer for archive extraction.
    pub fn for_extract(
        progress: Option<ProgressFn>,
        inner: W,
        path: impl Into<PathBuf>,
        total_bytes: Option<u64>,
    ) -> Self {
        let path = path.into();

        if let Some(progress) = &progress {
            progress(ProgressEvent::ExtractStarted {
                path: &path,
                total_bytes,
            });
        }

        Self {
            inner,
            progress,
            context: ProgressContext::Extract { path },
            written: 0,
            total_bytes,
        }
    }
}

impl<W> Write for ProgressWriter<W>
where
    W: Write,
{
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let n = self.inner.write(buf)?;
        self.written += n as u64;

        if let Some(progress) = &self.progress {
            match &self.context {
                ProgressContext::Download { url } => {
                    progress(ProgressEvent::DownloadProgress {
                        url,
                        bytes: self.written,
                        total_bytes: self.total_bytes,
                    });
                }
                ProgressContext::Extract { path } => {
                    progress(ProgressEvent::ExtractProgress {
                        path,
                        bytes: self.written,
                        total_bytes: self.total_bytes,
                    });
                }
            }
        }

        Ok(n)
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.inner.flush()
    }
}

impl<W> Drop for ProgressWriter<W> {
    fn drop(&mut self) {
        if let Some(progress) = &self.progress {
            match &self.context {
                ProgressContext::Download { url } => {
                    progress(ProgressEvent::DownloadComplete { url });
                }
                ProgressContext::Extract { path } => {
                    progress(ProgressEvent::ExtractComplete { path });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        path::Path,
        sync::{Arc, Mutex},
    };

    use super::*;

    fn capture_progress() -> (ProgressFn, Arc<Mutex<Vec<String>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);
        let progress = Arc::new(move |event: ProgressEvent<'_>| {
            let v = match event {
                ProgressEvent::DownloadStarted { url, total_bytes } => {
                    format!("dl-start:{url}:{total_bytes:?}")
                }
                ProgressEvent::DownloadProgress {
                    url,
                    bytes,
                    total_bytes,
                } => {
                    format!("dl-progress:{url}:{bytes}:{total_bytes:?}")
                }
                ProgressEvent::DownloadComplete { url } => {
                    format!("dl-complete:{url}")
                }
                ProgressEvent::ExtractStarted { path, total_bytes } => {
                    format!("ex-start:{}:{total_bytes:?}", path.display())
                }
                ProgressEvent::ExtractProgress {
                    path,
                    bytes,
                    total_bytes,
                } => {
                    format!("ex-progress:{}:{bytes}:{total_bytes:?}", path.display())
                }
                ProgressEvent::ExtractComplete { path } => {
                    format!("ex-complete:{}", path.display())
                }
            };
            events_clone.lock().unwrap().push(v);
        });

        (progress, events)
    }

    #[test]
    fn download_emits_start_progress_complete() {
        let (progress, events) = capture_progress();
        let mut buf = Vec::new();
        let url = Url::parse("http://example.com/file.pdb").unwrap();

        {
            let mut w = ProgressWriter::for_download(Some(progress), &mut buf, &url, Some(10));
            w.write_all(b"hello").unwrap();
            w.write_all(b"world").unwrap();
        }

        assert_eq!(buf, b"helloworld");
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0], "dl-start:http://example.com/file.pdb:Some(10)");
        assert_eq!(
            events[1],
            "dl-progress:http://example.com/file.pdb:5:Some(10)"
        );
        assert_eq!(
            events[2],
            "dl-progress:http://example.com/file.pdb:10:Some(10)"
        );
        assert_eq!(events[3], "dl-complete:http://example.com/file.pdb");
    }

    #[test]
    fn extract_emits_start_progress_complete() {
        let (progress, events) = capture_progress();
        let mut buf = Vec::new();

        {
            let mut w = ProgressWriter::for_extract(
                Some(progress),
                &mut buf,
                Path::new("/tmp/vmlinux"),
                Some(100),
            );
            w.write_all(b"data").unwrap();
        }

        assert_eq!(buf, b"data");
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], "ex-start:/tmp/vmlinux:Some(100)");
        assert_eq!(events[1], "ex-progress:/tmp/vmlinux:4:Some(100)");
        assert_eq!(events[2], "ex-complete:/tmp/vmlinux");
    }

    #[test]
    fn none_progress_is_passthrough() {
        let mut buf = Vec::new();
        let url = Url::parse("http://example.com/file.pdb").unwrap();

        {
            let mut w = ProgressWriter::for_download(None, &mut buf, &url, Some(10));
            w.write_all(b"hello").unwrap();
        }

        assert_eq!(buf, b"hello");
    }

    #[test]
    fn unknown_total_bytes() {
        let (progress, events) = capture_progress();
        let mut buf = Vec::new();
        let url = Url::parse("http://example.com/x").unwrap();
        {
            let mut w = ProgressWriter::for_download(Some(progress), &mut buf, &url, None);
            w.write_all(b"abc").unwrap();
        }
        let events = events.lock().unwrap();
        assert_eq!(events[0], "dl-start:http://example.com/x:None");
        assert_eq!(events[1], "dl-progress:http://example.com/x:3:None");
        assert_eq!(events[2], "dl-complete:http://example.com/x");
    }
}
