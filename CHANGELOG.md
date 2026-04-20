# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

### Added

### Fixed

## [0.6.0] - 2026-04-20

### Changed

- **Breaking:** `visit_struct` and `visit_struct_only` now take a fallible
  visitor (`FnMut(&StructField) -> Result<(), E>`) and return `Result<(), E>`,
  letting callers stop visiting early by returning an error.
- **Breaking:** `visit_struct_only` renamed to `visit_struct_schema`.

### Added

### Fixed

## [0.5.0] - 2026-04-17

### Changed

- **Breaking:** On-disk profile format switched to rkyv. Profiles are now
  stored as a single archived blob (`.isr`) and read via `mmap` without an
  explicit deserialize pass. The `Codec` trait and the `BincodeCodec`,
  `JsonCodec`, and `MsgpackCodec` codecs are removed, and `IsrCache` is no
  longer generic over a codec. Existing cached profiles from older versions
  must be regenerated.
- **Breaking:** `Profile` is now a typed view over the archived schema.
    - Fields on `Profile`, `Enum`, `Struct`, `Field`, `Array`, `Pointer`,
      `Bitfield`, `EnumRef`, and `StructRef` are no longer public; access
      them through accessor methods.
- **Breaking:** `Architecture` is now an enum.
- **Breaking:** `isr-dl-pdb` crate renamed to `isr-dl-windows`
  - The `isr::download::pdb` module renamed to `isr::download::windows`.
  - The `dl-pdb` feature renamed to `dl-windows`.
- **Breaking:** Windows symbol downloader redesigned.
- **Breaking:** Ubuntu symbol downloader redesigned.
- **Breaking:** `Error` types across downloader crates split into two
  layers: a shared `isr_dl::Error` and per-crate `DownloaderError` for internal
  specifics. `IsrCache::Error::Downloader` now wraps `isr_dl::Error`.
- **Breaking:** MSRV raised to `1.91.0`
- `IsrCache` uses atomic `.part` file rename for profile writes so a killed or
  failed run never leaves a half-written file in the cache.

### Added

- `IsrCache::with_progress(..)` hooks a `Fn(ProgressEvent<'_>)` callback
  into downloaders
  - Download and extraction both emit
    `DownloadStarted`/`DownloadProgress`/`DownloadComplete` and
    `ExtractStarted`/`ExtractProgress`/`ExtractComplete` events.
- `IsrCache::with_offline(true)` forces lookup-only behavior: if an
  artifact is not already cached, the call fails with
  `isr_dl::Error::ArtifactNotFound` and no network request is made.
- `IsrCache::download_from_codeview` and
  `IsrCache::download_from_image_signature` for fetching raw PDBs and PE
  binaries from Microsoft symbol servers without building a profile.
- Downloading PE images from Microsoft symbol servers via the new
  `ImageSignature` type.
- New `isr-dl` crate: shared download infrastructure used by both
  platform downloaders.
- `Profile::lookup_symbol(rva)` returns the `Symbol` whose RVA is closest
  to `rva` without exceeding it.
- `isr_core::schema` module exposing the owned profile data model, and
  an `isr_core::visit` module with a visitor over the archived profile.
- `examples/windows.rs` and `examples/ubuntu.rs` demonstrating end-to-end
  profile creation with an `indicatif`-backed progress bar.

### Fixed

- Profile creation no longer aborts on unknown or unimplemented PDB type
  kinds. Unparseable fields are skipped and parsing continues.
- Errors during profile creation clean up the partial output file instead
  of leaving a truncated `.isr` behind.
- Banner parsing and downloader errors from
  `IsrCache::entry_from_linux_banner` are now propagated instead of being
  collapsed to a generic failure.
- Additional `PrimitiveKind` variants are handled when computing type
  sizes in PDB parsing.
