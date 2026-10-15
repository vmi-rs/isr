//! Typedef naming, anonymous member and prototype tests against a
//! compiler-generated fixture.
//!
//! `tests/fixtures/c_types.o` is built from the sources next to it with
//! LLVM and the GNU linker of an arm64 Linux host, running the commands
//! below inside `tests/fixtures`:
//!
//! ```text
//! clang --target=aarch64-linux-gnu -std=gnu23 -g -O1 -fdebug-prefix-map=$PWD=. -c c_types_one.c -o c_types_one.o
//! clang --target=aarch64-linux-gnu -std=gnu23 -g -O1 -fdebug-prefix-map=$PWD=. -c c_types_two.c -o c_types_two.o
//! ld -r c_types_two.o c_types_one.o -o c_types.o
//! ```
//!
//! The units include two copies of the same header from different paths,
//! like XNU units include `osfmk/mach/message.h` or its installed copy.
//! With optimization, the first linked unit describes `add`, which it
//! calls, by a declaration whose parameters have no names, `square` is
//! inlined into `pick` and has an abstract instance, and the static
//! `hidden` only exists inlined.

use std::{convert::Infallible, fs::File, path::PathBuf};

use isr_core::schema::{Base, Profile, Type, Variant};

/// Returns the path of a file in `tests/fixtures`.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Builds a profile from `c_types.o` and an empty `System.map` and runs
/// `check` on it.
fn with_c_types_profile(check: impl FnOnce(&Profile)) {
    let kernel = File::open(fixture("c_types.o")).unwrap();
    let systemmap = File::open(fixture("empty.System.map")).unwrap();

    isr_dwarf::create_profile(kernel, systemmap, |profile| {
        check(profile);
        Ok::<(), Infallible>(())
    })
    .unwrap();
}

/// Returns the struct name of a struct-typed value, looking through
/// pointers.
fn struct_name(ty: &Type) -> &str {
    match ty {
        Type::Struct(udt) => &udt.name,
        Type::Pointer(pointer) => struct_name(&pointer.subtype),
        ty => panic!("expected struct, found {ty:?}"),
    }
}

/// Returns the names of the structs that have a field named `field`.
fn structs_with_field<'a>(profile: &'a Profile, field: &str) -> Vec<&'a str> {
    profile
        .structs
        .iter()
        .filter(|(_, udt)| udt.fields.contains_key(field))
        .map(|(name, _)| name.as_str())
        .collect()
}

#[test]
fn anonymous_definitions_take_typedef_names() {
    with_c_types_profile(|profile| {
        let header = &profile.structs["header_t"];
        assert_eq!(header.size, 16);
        assert_eq!(header.fields["bits"].offset, 0);
        assert_eq!(header.fields["size"].offset, 4);
        assert_eq!(header.fields["remote"].offset, 8);

        // The copies from both header paths collapse into one definition.
        assert_eq!(structs_with_field(profile, "bits"), ["header_t"]);

        let flags = &profile.enums["flags_t"];
        assert!(matches!(flags.subtype, Type::Base(Base::U32)));
        assert!(matches!(flags.fields["FLAG_WAIT"], Variant::U64(1)));
    });
}

#[test]
fn references_use_typedef_names() {
    with_c_types_profile(|profile| {
        let use_two = &profile.functions["use_two"];
        assert_eq!(struct_name(&use_two.parameters[0].ty), "header_t");

        let pick = &profile.functions["pick"];
        assert!(matches!(&pick.return_type, Type::Enum(flags) if flags.name == "flags_t"));
        assert_eq!(struct_name(&pick.parameters[1].ty), "header_t");

        let kmsg = &profile.structs["kmsg"];
        let union = &profile.structs[struct_name(&kmsg.fields["__unnamed_field_1"].ty)];
        let inner = &profile.structs[struct_name(&union.fields["__unnamed_field_1"].ty)];
        assert_eq!(struct_name(&inner.fields["kdata"].ty), "header_t");
    });
}

#[test]
fn typedef_name_collisions_keep_anonymous_names() {
    with_c_types_profile(|profile| {
        // `struct taken` keeps the name, the anonymous `taken` does not.
        assert_eq!(structs_with_field(profile, "named"), ["taken"]);
        let anonymous = structs_with_field(profile, "anonymous");
        assert_eq!(anonymous.len(), 1);
        assert!(anonymous[0].starts_with("__unnamed_"));

        // `conflict_t` names different anonymous structs in the two units.
        assert!(!profile.structs.contains_key("conflict_t"));
        for field in ["first", "second"] {
            let names = structs_with_field(profile, field);
            assert_eq!(names.len(), 1);
            assert!(names[0].starts_with("__unnamed_"), "{field} in {names:?}");
        }
    });
}

#[test]
fn anonymous_members_resolve_with_absolute_offsets() {
    with_c_types_profile(|profile| {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(profile).unwrap();
        let archived =
            rkyv::access::<isr_core::schema::ArchivedProfile, rkyv::rancor::Error>(&bytes).unwrap();
        let profile = isr_core::Profile::from_archived(archived);

        let kmsg = profile.find_struct("kmsg").unwrap();
        let offset = |name| kmsg.find_field(&profile, name).map(|field| field.offset());
        assert_eq!(offset("next"), Some(0));
        assert_eq!(offset("big_data"), Some(8));
        assert_eq!(offset("kdata"), Some(8));
        assert_eq!(offset("udata"), Some(16));
        assert_eq!(offset("aux_size"), Some(24));
        assert_eq!(offset("missing"), None);
        assert!(kmsg.field("kdata").is_none());
    });
}

#[test]
fn external_prototypes_are_collected() {
    with_c_types_profile(|profile| {
        let mut names = profile.functions.keys().collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["add", "pick", "square", "sum", "use_two"]);

        // The definition replaces the earlier declaration without names.
        let add = &profile.functions["add"];
        assert!(matches!(add.return_type, Type::Base(Base::I32)));
        assert!(!add.variadic);
        let parameters = add
            .parameters
            .iter()
            .map(|parameter| (parameter.name.as_deref(), &parameter.ty))
            .collect::<Vec<_>>();
        assert!(matches!(
            parameters[..],
            [
                (Some("left"), Type::Base(Base::I32)),
                (Some("right"), Type::Base(Base::I32))
            ]
        ));

        let sum = &profile.functions["sum"];
        assert!(sum.variadic);
        assert_eq!(sum.parameters.len(), 1);
        assert_eq!(sum.parameters[0].name.as_deref(), Some("count"));

        // The abstract instance of an inlined function describes it.
        let square = &profile.functions["square"];
        assert_eq!(square.parameters.len(), 1);
        assert_eq!(square.parameters[0].name.as_deref(), Some("value"));

        let pick = &profile.functions["pick"];
        assert_eq!(
            pick.parameters
                .iter()
                .map(|parameter| parameter.name.as_deref())
                .collect::<Vec<_>>(),
            [Some("kmsg"), Some("header")]
        );
        assert_eq!(struct_name(&pick.parameters[0].ty), "kmsg");
    });
}
