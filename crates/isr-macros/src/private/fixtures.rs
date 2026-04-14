//! Test fixtures.

use std::sync::OnceLock;

use indexmap::IndexMap;
use isr_core::{
    Profile,
    schema::{
        Architecture, ArchivedProfile, Array, Base, Bitfield, Field, Pointer, Struct, StructKind,
        StructRef, Type,
    },
};

type OwnedProfile = isr_core::schema::Profile;

/// Returns a synthetic [`Profile`] that mirrors a subset of Windows 10
/// `ntkrnlmp.pdb` (build 10.0.18362.356, `CodeView` GUID
/// `ce7ffb00c20b87500211456b3e905c471`).
///
/// Contains only the symbols and struct fields exercised by the [`symbols!`]
/// and [`offsets!`] doctests. Offsets and sizes match the real PDB.
///
/// The `CodeView` record identifying this kernel is:
///
/// ```no_run
/// # use isr::download::windows::CodeView;
/// let codeview = CodeView {
///     name: "ntkrnlmp.pdb".into(),
///     guid: "ce7ffb00c20b87500211456b3e905c47".into(),
///     age: 1,
/// };
/// ```
///
/// In production code, the full profile is obtained via `IsrCache`:
///
/// ```no_run
/// use isr::{download::windows::CodeView, IsrCache};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let cache = IsrCache::new("cache")?;
/// let entry = cache.entry_from_codeview(CodeView {
///     name: "ntkrnlmp.pdb".into(),
///     guid: "ce7ffb00c20b87500211456b3e905c47".into(),
///     age: 1,
/// })?;
/// let profile = entry.profile()?;
/// # Ok(())
/// # }
/// ```
///
/// [`symbols!`]: crate::symbols
/// [`offsets!`]: crate::offsets
#[doc(hidden)]
pub fn ntkrnlmp_profile() -> Profile<'static> {
    static BYTES: OnceLock<rkyv::util::AlignedVec> = OnceLock::new();

    let bytes: &'static [u8] = BYTES
        .get_or_init(|| {
            rkyv::to_bytes::<rkyv::rancor::Error>(&build_ntkrnlmp_subset())
                .expect("serialize fixture profile")
        })
        .as_ref();

    let archived = rkyv::access::<ArchivedProfile, rkyv::rancor::Error>(bytes)
        .expect("access fixture profile");

    Profile::from_archived(archived)
}

fn build_ntkrnlmp_subset() -> OwnedProfile {
    let mut symbols = IndexMap::new();
    symbols.insert("PsActiveProcessHead".to_owned(), 0x0043_7BC0);
    symbols.insert("PsInitialSystemProcess".to_owned(), 0x0057_33A0);
    // Stored under the Shadow alias to exercise symbols!'s alias resolution.
    symbols.insert("KiSystemCall64Shadow".to_owned(), 0x0040_6480);
    // Stored under the decorated alias to exercise multi-alias resolution.
    symbols.insert("_NtOpenFile@24".to_owned(), 0x006C_3D00);

    let mut structs = IndexMap::new();

    structs.insert(
        "_EX_FAST_REF".to_owned(),
        Struct {
            kind: StructKind::Struct,
            size: 8,
            fields: field_map([
                (
                    "RefCnt",
                    Field {
                        offset: 0,
                        ty: Type::Bitfield(Bitfield {
                            subtype: Box::new(Type::Base(Base::U64)),
                            bit_length: 4,
                            bit_position: 0,
                        }),
                    },
                ),
                (
                    "Value",
                    Field {
                        offset: 0,
                        ty: Type::Base(Base::U64),
                    },
                ),
            ]),
        },
    );

    // Affinity lives here so offsets! exercises its nested-field lookup
    // (_EPROCESS -> Pcb: _KPROCESS -> Affinity).
    structs.insert(
        "_KPROCESS".to_owned(),
        Struct {
            kind: StructKind::Struct,
            size: 440,
            fields: field_map([(
                "Affinity",
                Field {
                    offset: 80,
                    ty: Type::Array(Array {
                        subtype: Box::new(Type::Base(Base::U8)),
                        dims: vec![168],
                    }),
                },
            )]),
        },
    );

    structs.insert(
        "_EPROCESS".to_owned(),
        Struct {
            kind: StructKind::Struct,
            size: 2176,
            fields: field_map([
                (
                    "Pcb",
                    Field {
                        offset: 0,
                        ty: Type::Struct(StructRef {
                            name: "_KPROCESS".to_owned(),
                        }),
                    },
                ),
                (
                    "UniqueProcessId",
                    Field {
                        offset: 744,
                        ty: void_ptr(),
                    },
                ),
                (
                    "WoW64Process",
                    Field {
                        offset: 1064,
                        ty: void_ptr(),
                    },
                ),
            ]),
        },
    );

    // Stored under the _K prefix to exercise offsets!'s struct-alias resolution.
    structs.insert(
        "_KLDR_DATA_TABLE_ENTRY".to_owned(),
        Struct {
            kind: StructKind::Struct,
            size: 296,
            fields: field_map([
                (
                    "InLoadOrderLinks",
                    Field {
                        offset: 0,
                        ty: Type::Array(Array {
                            subtype: Box::new(Type::Base(Base::U8)),
                            dims: vec![16],
                        }),
                    },
                ),
                (
                    "DllBase",
                    Field {
                        offset: 48,
                        ty: void_ptr(),
                    },
                ),
                (
                    "FullDllName",
                    Field {
                        offset: 72,
                        ty: Type::Array(Array {
                            subtype: Box::new(Type::Base(Base::U8)),
                            dims: vec![16],
                        }),
                    },
                ),
            ]),
        },
    );

    OwnedProfile {
        architecture: Architecture::Amd64,
        enums: IndexMap::new(),
        structs,
        symbols,
    }
}

fn void_ptr() -> Type {
    Type::Pointer(Pointer {
        subtype: Box::new(Type::Base(Base::Void)),
        size: 8,
    })
}

fn field_map<const N: usize>(entries: [(&str, Field); N]) -> IndexMap<String, Field> {
    entries
        .into_iter()
        .map(|(name, field)| (name.to_owned(), field))
        .collect()
}
