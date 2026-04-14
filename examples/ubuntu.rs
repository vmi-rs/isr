//! Downloads an Ubuntu kernel + debug symbols via `IsrCache`, then prints
//! symbol addresses and struct offsets resolved from the profile.
//!
//! Uses a hardcoded banner for the Ubuntu 6.8.0-40.40~22.04.3-generic kernel.
//! Download + extraction progress is rendered via `indicatif`.

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
        _text: u64,
        init_task: u64,
        entry_SYSCALL_64: u64,
        pcpu_hot: u64,
    }
}

offsets! {
    #[derive(Debug)]
    pub struct Offsets {
        struct pcpu_hot {
            current_task: Field,
        }

        struct fs_struct {
            root: Field, // struct path root;
            pwd: Field,  // struct path pwd;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache = IsrCache::new("cache")?.with_progress(indicatif_progress());

    // Use the Linux banner of the Ubuntu 6.8.0-40.40~22.04.3-generic kernel.
    let banner = "Linux version 6.8.0-40-generic \
                  (buildd@lcy02-amd64-078) \
                  (x86_64-linux-gnu-gcc-12 (Ubuntu 12.3.0-1ubuntu1~22.04) \
                  12.3.0, GNU ld (GNU Binutils for Ubuntu) 2.38) \
                  #40~22.04.3-Ubuntu SMP PREEMPT_DYNAMIC \
                  Tue Jul 30 17:30:19 UTC 2 \
                  (Ubuntu 6.8.0-40.40~22.04.3-generic 6.8.12)";

    let entry = cache.entry_from_linux_banner(banner)?;
    let profile = entry.profile()?;

    let symbols = Symbols::new(&profile)?;
    let offsets = Offsets::new(&profile)?;

    println!("{symbols:#x?}");
    println!("{offsets:#x?}");

    Ok(())
}

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

fn new_bar(style: &ProgressStyle, total_bytes: Option<u64>) -> ProgressBar {
    match total_bytes {
        Some(total_bytes) => ProgressBar::new(total_bytes).with_style(style.clone()),
        None => ProgressBar::new_spinner(),
    }
}

fn basename(s: &str) -> &str {
    s.rsplit_once(['/', '\\']).map_or(s, |(_, rest)| rest)
}
