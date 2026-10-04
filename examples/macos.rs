//! Downloads a macOS Kernel Debug Kit via `IsrCache`, then prints symbol
//! addresses and struct offsets resolved from the XNU kernel profile.
//!
//! Takes the XNU kernel version string as the first argument, e.g.
//! `Darwin Kernel Version 27.0.0: Tue Aug 11 21:00:07 PDT 2026; root:xnu-13432.1.9~1/RELEASE_ARM64_VMAPPLE`.
//! An optional second argument overrides the cache directory (default:
//! `cache`). Download + extraction progress is rendered via `indicatif`.

use std::{collections::HashMap, sync::Mutex};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use isr::{
    IsrCache,
    download::ProgressEvent,
    macros::{Field, offsets, symbols},
};

symbols! {
    #[derive(Debug)]
    pub struct Symbols {
        allproc: u64,
        kernproc: u64,
        version: u64,
    }
}

offsets! {
    #[derive(Debug)]
    pub struct Offsets {
        struct proc {
            p_pid: Field,
            p_comm: Field,
        }

        struct task {
            map: Field,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let version = match args.next() {
        Some(version) => version,
        None => return Err("usage: macos <darwin kernel version string> [cache directory]".into()),
    };
    let cache_directory = args.next().unwrap_or_else(|| String::from("cache"));

    let cache = IsrCache::new(cache_directory)?.with_progress(indicatif_progress());

    let entry = cache.entry_from_darwin_version(&version, None)?;
    let profile = entry.profile()?;

    let symbols = Symbols::new(&profile)?;
    let offsets = Offsets::new(&profile)?;

    println!("{symbols:#x?}");
    println!("{offsets:#x?}");

    Ok(())
}

/// Returns a progress callback that renders download and extraction progress
/// bars.
fn indicatif_progress() -> impl Fn(ProgressEvent<'_>) + Send + Sync + 'static {
    let multi = MultiProgress::new();
    let bars = Mutex::new(HashMap::<String, ProgressBar>::new());

    let style = ProgressStyle::with_template(
        "{msg:<24} [{bar:40.cyan/blue}] {bytes:>10}/{total_bytes:<10} {elapsed_precise}",
    )
    .unwrap()
    .progress_chars("=> ");

    move |event| match event {
        ProgressEvent::DownloadStarted { url, total_bytes } => {
            let bar = multi.add(new_bar(&style, total_bytes));
            bar.set_message(format!("downloading {}", basename(url.as_str())));
            bars.lock().unwrap().insert(url.to_string(), bar);
        }
        ProgressEvent::DownloadProgress { url, bytes, .. } => {
            if let Some(bar) = bars.lock().unwrap().get(url.as_str()) {
                bar.set_position(bytes);
            }
        }
        ProgressEvent::DownloadComplete { url } => {
            if let Some(bar) = bars.lock().unwrap().remove(url.as_str()) {
                bar.finish();
            }
        }
        ProgressEvent::ExtractStarted { path, total_bytes } => {
            let key = path.display().to_string();
            let bar = multi.add(new_bar(&style, total_bytes));
            bar.set_message(format!("extracting {}", basename(&key)));
            bars.lock().unwrap().insert(key, bar);
        }
        ProgressEvent::ExtractProgress { path, bytes, .. } => {
            let key = path.display().to_string();
            if let Some(bar) = bars.lock().unwrap().get(&key) {
                bar.set_position(bytes);
            }
        }
        ProgressEvent::ExtractComplete { path } => {
            let key = path.display().to_string();
            if let Some(bar) = bars.lock().unwrap().remove(&key) {
                bar.finish();
            }
        }
    }
}

/// Creates a progress bar for `total_bytes`, or a spinner when the size is
/// unknown.
fn new_bar(style: &ProgressStyle, total_bytes: Option<u64>) -> ProgressBar {
    match total_bytes {
        Some(total_bytes) => ProgressBar::new(total_bytes).with_style(style.clone()),
        None => ProgressBar::new_spinner(),
    }
}

/// Returns the last component of a URL or path.
fn basename(s: &str) -> &str {
    s.rsplit_once(['/', '\\']).map_or(s, |(_, rest)| rest)
}
