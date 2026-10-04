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

/// Returns a universal binary with an x86_64 placeholder slice followed by
/// `arm64` as the arm64 slice.
fn universal_binary(arm64: &[u8], arm64_cputype: u32) -> Vec<u8> {
    const X86_64_OFFSET: u32 = 0x1000;
    const ARM64_OFFSET: u32 = 0x2000;
    let x86_64 = b"x86_64 placeholder";

    let mut data = Vec::new();
    data.extend_from_slice(&0xcafe_babe_u32.to_be_bytes());
    data.extend_from_slice(&2_u32.to_be_bytes());
    for (cputype, offset, size) in [
        (0x0100_0007, X86_64_OFFSET, x86_64.len()),
        (arm64_cputype, ARM64_OFFSET, arm64.len()),
    ] {
        data.extend_from_slice(&u32::to_be_bytes(cputype));
        data.extend_from_slice(&0_u32.to_be_bytes());
        data.extend_from_slice(&offset.to_be_bytes());
        data.extend_from_slice(&u32::try_from(size).unwrap().to_be_bytes());
        data.extend_from_slice(&12_u32.to_be_bytes());
    }

    data.resize(X86_64_OFFSET as usize, 0);
    data.extend_from_slice(x86_64);
    data.resize(ARM64_OFFSET as usize, 0);
    data.extend_from_slice(arm64);
    data
}

#[test]
fn universal_binary_uuid_comes_from_arm64_slice() {
    let thin = std::fs::read(fixture("macho_arm64")).unwrap();
    let uuid = isr_dwarf::macho_uuid(&thin).unwrap();
    assert!(uuid.is_some());

    let fat = universal_binary(&thin, 0x0100_000c);
    assert_eq!(isr_dwarf::macho_uuid(&fat).unwrap(), uuid);
}

#[test]
fn universal_binary_without_arm64_slice_is_rejected() {
    let thin = std::fs::read(fixture("macho_arm64")).unwrap();

    // Both slices claim to be x86_64.
    let fat = universal_binary(&thin, 0x0100_0007);
    assert!(matches!(
        isr_dwarf::macho_uuid(&fat),
        Err(Error::MissingArm64Slice)
    ));
}
