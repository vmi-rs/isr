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
        let detail = &profile.structs["Lazy::Detail"];
        assert_eq!(detail.size, 8);
        assert_eq!(detail.fields["d2"].offset, 4);

        let uses_nested = &profile.structs["UsesNested"];
        assert_eq!(
            struct_name(&uses_nested.fields["detail"].ty),
            "Lazy::Detail"
        );
    });
}

#[test]
fn nested_definition_inside_duplicate_is_added() {
    with_nested_scope_profile(|profile| {
        let hidden = &profile.structs["Dup::Hidden"];
        assert_eq!(hidden.size, 8);
        assert!(matches!(hidden.fields["h"].ty, Type::Base(Base::I64)));

        let uses_nested = &profile.structs["UsesNested"];
        assert_eq!(struct_name(&uses_nested.fields["hidden"].ty), "Dup::Hidden");
    });
}

#[test]
fn same_named_nested_types_keep_their_parents() {
    with_nested_scope_profile(|profile| {
        assert!(!profile.structs.contains_key("ExpansionData"));
        assert_eq!(profile.structs["Alpha::ExpansionData"].size, 4);
        assert_eq!(profile.structs["Beta::ExpansionData"].size, 16);

        let alpha = &profile.structs["Alpha"];
        let beta = &profile.structs["Beta"];
        assert_eq!(
            struct_name(&alpha.fields["reserved"].ty),
            "Alpha::ExpansionData"
        );
        assert_eq!(
            struct_name(&beta.fields["reserved"].ty),
            "Beta::ExpansionData"
        );
    });
}

#[test]
fn namespaces_qualify_type_names() {
    with_nested_scope_profile(|profile| {
        let scoped = &profile.structs["ns::Scoped"];
        assert_eq!(struct_name(&scoped.fields["leaf"].ty), "ns::Scoped::Leaf");
        assert_eq!(profile.structs["ns::Scoped::Leaf"].size, 2);

        let uses_nested = &profile.structs["UsesNested"];
        assert_eq!(struct_name(&uses_nested.fields["scoped"].ty), "ns::Scoped");
    });
}

#[test]
fn c_nested_types_keep_file_scope_names() {
    with_nested_scope_profile(|profile| {
        let outer = &profile.structs["COuter"];
        assert_eq!(struct_name(&outer.fields["inner"].ty), "CInner");
        assert_eq!(profile.structs["CInner"].size, 4);
        assert!(!profile.structs.contains_key("COuter::CInner"));
    });
}

#[test]
fn method_definitions_are_indexed_by_linkage_name() {
    with_nested_scope_profile(|profile| {
        let mut names = profile.functions.keys().collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["_ZN4LazyD0Ev", "_ZN4LazyD2Ev"]);

        // The definitions take their visibility from the declaration in
        // `Lazy` and carry the `this` parameter themselves.
        let destructor = &profile.functions["_ZN4LazyD2Ev"];
        assert!(matches!(destructor.return_type, Type::Base(Base::Void)));
        assert_eq!(destructor.parameters.len(), 1);
        assert_eq!(destructor.parameters[0].name.as_deref(), Some("this"));
        assert_eq!(struct_name(&destructor.parameters[0].ty), "Lazy");
    });
}
