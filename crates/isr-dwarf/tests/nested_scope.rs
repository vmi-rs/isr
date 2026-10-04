//! Nested type tests against a compiler-generated fixture.
//!
//! `tests/fixtures/nested_scope.o` is built from the sources next to it with
//! LLVM, running the commands below inside `tests/fixtures`:
//!
//! ```text
//! clang --target=x86_64-linux-gnu -g -O0 -fdebug-prefix-map=$PWD=. -c nested_scope_one.cpp -o one.o
//! clang --target=x86_64-linux-gnu -g -O0 -fdebug-prefix-map=$PWD=. -c nested_scope_two.cpp -o two.o
//! clang --target=x86_64-linux-gnu -g -O0 -fdebug-prefix-map=$PWD=. -c nested_scope_c.c -o c.o
//! ld.lld -r one.o two.o c.o -o nested_scope.o
//! ```
//!
//! Without link-time optimization every unit keeps its own copy of the
//! types it uses. Clang emits C++ nested types lazily, so only the copy of
//! `Dup` in the second unit carries `Dup::Hidden`, and the second unit
//! describes `Lazy`, whose key function lives in the first unit, as a
//! declaration that still defines `Lazy::Detail`.

use std::{convert::Infallible, fs::File, path::PathBuf};

use isr_core::schema::{Base, Profile, Type};

/// Returns the path of a file in `tests/fixtures`.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Builds a profile from `nested_scope.o` and an empty `System.map` and runs
/// `check` on it.
fn with_nested_scope_profile(check: impl FnOnce(&Profile)) {
    let kernel = File::open(fixture("nested_scope.o")).unwrap();
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

#[test]
fn nested_definition_inside_declaration_is_added() {
    with_nested_scope_profile(|profile| {
        let detail = &profile.structs["Detail"];
        assert_eq!(detail.size, 8);
        assert_eq!(detail.fields["d2"].offset, 4);

        let uses_nested = &profile.structs["UsesNested"];
        assert_eq!(struct_name(&uses_nested.fields["detail"].ty), "Detail");
    });
}

#[test]
fn nested_definition_inside_duplicate_is_added() {
    with_nested_scope_profile(|profile| {
        let hidden = &profile.structs["Hidden"];
        assert_eq!(hidden.size, 8);
        assert!(matches!(hidden.fields["h"].ty, Type::Base(Base::I64)));

        let uses_nested = &profile.structs["UsesNested"];
        assert_eq!(struct_name(&uses_nested.fields["hidden"].ty), "Hidden");
    });
}
