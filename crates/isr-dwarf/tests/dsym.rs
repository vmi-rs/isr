//! dSYM profile tests against a compiler-generated fixture.
//!
//! `tests/fixtures/macho_arm64` is built from the sources next to it with
//! LLVM, running the commands below inside `tests/fixtures`:
//!
//! ```text
//! clang --target=arm64-apple-macos14 -g -O0 -fdebug-prefix-map=$PWD=. -c macho_arm64_a.c -o macho_arm64_a.o
//! clang --target=arm64-apple-macos14 -g -O0 -fdebug-prefix-map=$PWD=. -c macho_arm64_b.c -o macho_arm64_b.o
//! ld64.lld -arch arm64 -platform_version macos 14.0 14.0 -e _main -oso_prefix $PWD \
//!     -o macho_arm64 macho_arm64_a.o macho_arm64_b.o
//! ```
//!
//! The executable carries a symbol table with debugger (STAB) entries and
//! the segments of a linked image, but no DWARF sections.

use std::{convert::Infallible, fs::File, path::PathBuf};

use isr_core::schema::{Architecture, Profile};
use isr_dwarf::Error;

/// Returns the path of a file in `tests/fixtures`.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Builds a profile from the `macho_arm64` executable and runs `check` on it.
fn with_macho_profile(check: impl FnOnce(&Profile)) {
    let dsym = File::open(fixture("macho_arm64")).unwrap();

    isr_dwarf::create_profile_from_dsym(dsym, |profile| {
        check(profile);
        Ok::<(), Infallible>(())
    })
    .unwrap();
}

#[test]
fn dsym_symbols_lose_one_underscore_and_prefer_globals() {
    with_macho_profile(|profile| {
        assert!(matches!(profile.architecture, Architecture::Arm64));

        // `counter` is both a static in one unit and a global in the other.
        assert_eq!(profile.symbols["counter"], 0x1_0000_4008);
        assert_eq!(profile.symbols["_leading"], 0x1_0000_4000);
        assert_eq!(profile.symbols["_mh_execute_header"], 0x1_0000_0000);
        assert_eq!(profile.symbols["counter_a"], 0x1_0000_0378);
        assert_eq!(profile.symbols["main"], 0x1_0000_0384);

        // Debugger (STAB) entries do not become symbols.
        assert_eq!(profile.symbols.len(), 5);
    });
}

#[test]
fn dsym_segments_come_from_load_commands() {
    with_macho_profile(|profile| {
        let segments = profile
            .segments
            .iter()
            .map(|(name, segment)| (name.as_str(), segment.address, segment.size))
            .collect::<Vec<_>>();

        assert_eq!(
            segments,
            [
                ("__PAGEZERO", 0, 0x1_0000_0000),
                ("__TEXT", 0x1_0000_0000, 0x4000),
                ("__DATA", 0x1_0000_4000, 0x4000),
                ("__LINKEDIT", 0x1_0000_8000, 0x3f0),
            ]
        );
    });
}

#[test]
fn dsym_rejects_non_macho_files() {
    let file = File::open(fixture("cross_unit.o")).unwrap();
    let result = isr_dwarf::create_profile_from_dsym(file, |_| Ok::<(), Infallible>(()));

    assert!(matches!(result, Err(Error::UnsupportedFormat(_))));
}
