//! Type extraction tests against a compiler-generated fixture.
//!
//! `tests/fixtures/cross_unit.o` is built from the sources next to it with
//! LLVM, running the commands below inside `tests/fixtures`:
//!
//! ```text
//! clang++ --target=x86_64-linux-gnu -flto -g -fdebug-prefix-map=$PWD=. -c cross_unit_one.cpp -o one.bc
//! clang++ --target=x86_64-linux-gnu -flto -g -fdebug-prefix-map=$PWD=. -c cross_unit_two.cpp -o two.bc
//! ld.lld -r one.bc two.bc -o cross_unit.o
//! ```
//!
//! Link-time optimization merges both C++ units into one module. The types
//! shared by the units are emitted once and the other unit refers to them
//! through `DW_FORM_ref_addr`, as `dsymutil` does for the XNU kernel.

use std::{convert::Infallible, fs::File, path::PathBuf};

use isr_core::schema::{Architecture, Base, Profile, Type};

/// Returns the path of a file in `tests/fixtures`.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Builds a profile from `cross_unit.o` and an empty `System.map` and runs
/// `check` on it.
fn with_cross_unit_profile(check: impl FnOnce(&Profile)) {
    let kernel = File::open(fixture("cross_unit.o")).unwrap();
    let systemmap = File::open(fixture("empty.System.map")).unwrap();

    isr_dwarf::create_profile(kernel, systemmap, |profile| {
        check(profile);
        Ok::<(), Infallible>(())
    })
    .unwrap();
}

/// Returns the struct name of a struct-typed value.
fn struct_name(ty: &Type) -> &str {
    match ty {
        Type::Struct(udt) => &udt.name,
        ty => panic!("expected struct, found {ty:?}"),
    }
}

#[test]
fn cross_unit_references_resolve_in_owning_unit() {
    with_cross_unit_profile(|profile| {
        assert!(matches!(profile.architecture, Architecture::Amd64));

        let two_only = &profile.structs["TwoOnly"];
        assert_eq!(struct_name(&two_only.fields["s"].ty), "Shared");
        assert_eq!(struct_name(&two_only.fields["inner"].ty), "Inner");
        assert_eq!(two_only.fields["inner"].offset, 32);
        assert!(matches!(two_only.fields["y"].ty, Type::Base(Base::I64)));
    });
}

#[test]
fn pointers_and_references_default_to_address_size() {
    with_cross_unit_profile(|profile| {
        let one_only = &profile.structs["OneOnly"];

        match &one_only.fields["s"].ty {
            Type::Pointer(pointer) => {
                assert_eq!(pointer.size, 8);
                assert_eq!(struct_name(&pointer.subtype), "Shared");
            }
            ty => panic!("expected pointer, found {ty:?}"),
        }

        match &one_only.fields["ref"].ty {
            Type::Pointer(pointer) => {
                assert_eq!(pointer.size, 8);
                assert!(matches!(*pointer.subtype, Type::Base(Base::I32)));
            }
            ty => panic!("expected pointer, found {ty:?}"),
        }
    });
}

#[test]
fn nested_definitions_are_added() {
    with_cross_unit_profile(|profile| {
        let inner = &profile.structs["Inner"];
        assert_eq!(inner.size, 16);
        assert_eq!(inner.fields["a"].offset, 0);
        assert_eq!(inner.fields["b"].offset, 8);

        let shared = &profile.structs["Shared"];
        let anonymous = &shared.fields["__unnamed_field_1"];
        assert_eq!(anonymous.offset, 16);

        let union = &profile.structs[struct_name(&anonymous.ty)];
        assert!(matches!(union.fields["i"].ty, Type::Base(Base::I32)));
        assert!(matches!(union.fields["f"].ty, Type::Base(Base::F32)));

        assert_eq!(
            shared.fields.keys().collect::<Vec<_>>(),
            ["inner", "__unnamed_field_1", "next"],
            "static members and methods are not fields"
        );
    });
}

#[test]
fn every_struct_reference_resolves() {
    with_cross_unit_profile(|profile| {
        for (name, udt) in &profile.structs {
            for (field_name, field) in &udt.fields {
                let mut ty = &field.ty;
                while let Type::Pointer(pointer) = ty {
                    ty = &pointer.subtype;
                }

                if let Type::Struct(target) = ty {
                    assert!(
                        profile.structs.contains_key(&target.name),
                        "{name}.{field_name} refers to missing {}",
                        target.name
                    );
                }
            }
        }
    });
}

#[test]
fn linux_profiles_have_no_segments() {
    with_cross_unit_profile(|profile| {
        assert!(profile.segments.is_empty());
    });
}
