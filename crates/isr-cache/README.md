# Opinionated cache for OS kernel profiles

This crate provides a caching mechanism for profiles generated and used by
the [`isr`] crate family. It offers several features to streamline the process
of accessing and managing symbol information, including methods for
downloading necessary debug symbols for Windows (PDB files) and Linux
(DWARF debug info and system map).

## Usage

The main component of this crate is the [`IsrCache`] struct.

Example of loading a profile from a PDB file using the CodeView information:

```rust,ignore
use isr::{
    download::pdb::CodeView,
    cache::{IsrCache, JsonCodec},
};

// Create a new cache instance.
let cache = IsrCache::<JsonCodec>::new("cache")?;

// Use the CodeView information of the Windows 10.0.18362.356 kernel.
let codeview = CodeView {
    path: String::from("ntkrnlmp.pdb"),
    guid: String::from("ce7ffb00c20b87500211456b3e905c471"),
};

// Fetch and create (or get existing) the entry.
let entry = cache.entry_from_codeview(codeview)?;

// Get the profile from the entry.
let profile = entry.profile()?;
```

Example of loading a profile based on a Linux kernel banner:

```rust,ignore
use isr::cache::{IsrCache, JsonCodec};

// Create a new cache instance.
let cache = IsrCache::<JsonCodec>::new("cache")?;

// Use the Linux banner of the Ubuntu 6.8.0-40.40~22.04.3-generic kernel.
let banner = "Linux version 6.8.0-40-generic \
              (buildd@lcy02-amd64-078) \
              (x86_64-linux-gnu-gcc-12 (Ubuntu 12.3.0-1ubuntu1~22.04) \
              12.3.0, GNU ld (GNU Binutils for Ubuntu) 2.38) \
              #40~22.04.3-Ubuntu SMP PREEMPT_DYNAMIC \
              Tue Jul 30 17:30:19 UTC 2 \
              (Ubuntu 6.8.0-40.40~22.04.3-generic 6.8.12)";

// Fetch and create (or get existing) the entry.
// Note that the download of Linux debug symbols may take a while.
let entry = cache.entry_from_linux_banner(banner)?;

// Get the profile from the entry.
let profile = entry.profile()?;
```

Consult the [`vmi`] crate for more information on how to download debug
symbols for introspected VMs.


License: MIT

[`isr`]: https://docs.rs/isr/latest/isr/index.html
[`vmi`]: https://docs.rs/vmi/latest/vmi/index.html
