use crate::Error;

/// A field within a structure.
///
/// `Field` encapsulates the offset and size of a field, enabling type-safe
/// access to structure members. It's primarily used with the [`offsets!`] macro
/// for defining structure layouts and accessing their fields.
///
/// [`offsets!`]: crate::offsets
#[derive(Debug, Clone, Copy)]
pub struct Field {
    /// The offset of the field from the beginning of the structure, in bytes.
    pub offset: u64,

    /// The size of the field, in bytes.
    pub size: u64,
}

/// A bitfield within a structure.
///
/// `Bitfield` provides information about the offset, size, bit position, and
/// bit length of a bitfield member. It extends the functionality of [`Field`]
/// by allowing access to individual bits within a field.
#[derive(Debug, Clone, Copy)]
pub struct Bitfield {
    /// The offset of the bitfield from the beginning of the structure, in bytes.
    pub offset: u64,

    /// The size of the underlying field containing the bitfield, in bytes.
    pub size: u64,

    /// The starting bit position of the bitfield within the underlying field.
    pub bit_position: u64,

    /// The length of the bitfield, in bits.
    pub bit_length: u64,
}

impl Bitfield {
    /// Extracts the bitfield value from a given integer.
    ///
    /// This method performs bitwise operations to isolate and return the
    /// value represented by the bitfield within the provided integer.
    pub fn value_from(&self, value: u64) -> u64 {
        let result = value >> self.bit_position;
        let result = result & ((1 << self.bit_length) - 1);

        #[expect(clippy::let_and_return)]
        result
    }
}

/// A field descriptor.
///
/// This descriptor can be either a [`Field`] or a [`Bitfield`].
#[derive(Debug, Clone)]
pub enum FieldDescriptor {
    /// Represents a regular field.
    Field(Field),

    /// Represents a bitfield.
    Bitfield(Bitfield),
}

impl FieldDescriptor {
    /// Returns the offset of the field or bitfield, in bytes.
    pub fn offset(&self) -> u64 {
        match self {
            FieldDescriptor::Field(field) => field.offset,
            FieldDescriptor::Bitfield(bitfield) => bitfield.offset,
        }
    }

    /// Returns the size of the field or bitfield, in bytes.
    pub fn size(&self) -> u64 {
        match self {
            FieldDescriptor::Field(field) => field.size,
            FieldDescriptor::Bitfield(bitfield) => bitfield.size,
        }
    }
}

impl TryFrom<FieldDescriptor> for u64 {
    type Error = Error;

    fn try_from(value: FieldDescriptor) -> Result<Self, Self::Error> {
        match value {
            FieldDescriptor::Field(field) => Ok(field.offset),
            FieldDescriptor::Bitfield(bitfield) => Ok(bitfield.offset),
        }
    }
}

impl TryFrom<FieldDescriptor> for Field {
    type Error = Error;

    fn try_from(value: FieldDescriptor) -> Result<Self, Self::Error> {
        match value {
            FieldDescriptor::Field(field) => Ok(field),
            FieldDescriptor::Bitfield(_) => {
                Err(Error::Conversion("expected field, found bitfield"))
            }
        }
    }
}

impl TryFrom<FieldDescriptor> for Bitfield {
    type Error = Error;

    fn try_from(value: FieldDescriptor) -> Result<Self, Self::Error> {
        match value {
            FieldDescriptor::Field(_) => Err(Error::Conversion("expected bitfield, found field")),
            FieldDescriptor::Bitfield(bitfield) => Ok(bitfield),
        }
    }
}

//
//
//

pub trait IntoField<T> {
    type Error;

    fn into_field(self) -> Result<T, Error>;
}

impl IntoField<u64> for Result<FieldDescriptor, Error> {
    type Error = Error;

    fn into_field(self) -> Result<u64, Error> {
        self?.try_into()
    }
}

impl IntoField<Field> for Result<FieldDescriptor, Error> {
    type Error = Error;

    fn into_field(self) -> Result<Field, Error> {
        self?.try_into()
    }
}

impl IntoField<Bitfield> for Result<FieldDescriptor, Error> {
    type Error = Error;

    fn into_field(self) -> Result<Bitfield, Error> {
        self?.try_into()
    }
}

impl IntoField<Option<u64>> for Result<FieldDescriptor, Error> {
    type Error = Error;

    fn into_field(self) -> Result<Option<u64>, Error> {
        match self {
            Ok(descriptor) => Ok(Some(descriptor.try_into()?)),
            Err(_) => Ok(None),
        }
    }
}

impl IntoField<Option<Field>> for Result<FieldDescriptor, Error> {
    type Error = Error;

    fn into_field(self) -> Result<Option<Field>, Error> {
        match self {
            Ok(descriptor) => Ok(Some(descriptor.try_into()?)),
            Err(_) => Ok(None),
        }
    }
}

impl IntoField<Option<Bitfield>> for Result<FieldDescriptor, Error> {
    type Error = Error;

    fn into_field(self) -> Result<Option<Bitfield>, Error> {
        match self {
            Ok(descriptor) => Ok(Some(descriptor.try_into()?)),
            Err(_) => Ok(None),
        }
    }
}

/// Defines offsets for members within a structure.
///
/// This macro facilitates type-safe access to structure members in the ISR
/// framework, automatically calculating field offsets and sizes based on
/// provided profile data.
///
/// # Usage
///
/// ```rust
/// # use isr::{
/// #     cache::{Codec as _, JsonCodec},
/// #     macros::{offsets, Bitfield, Field},
/// # };
/// #
/// offsets! {
///     // Defined attributes are applied to each substucture.
///     #[derive(Debug)]
///     pub struct Offsets {
///         struct _EX_FAST_REF {
///             RefCnt: Bitfield,
///             Value: Field,
///         }
///
///         struct _EPROCESS {
///             UniqueProcessId: Field,
///
///             // Define an alternative name for a field.
///             #[isr(alias = "Wow64Process")]
///             WoW64Process: Field,
///
///             // We can even define field names that are present
///             // in the nested structures.
///             Affinity: Field,  // Defined in _KPROCESS
///         }
///
///         // Define an alternative name for a structure.
///         #[isr(alias = "_KLDR_DATA_TABLE_ENTRY")]
///         struct _LDR_DATA_TABLE_ENTRY {
///             InLoadOrderLinks: Field,
///             DllBase: Field,
///             FullDllName: Field,
///         }
///     }
/// }
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Use the profile of a Windows 10.0.18362.356 kernel.
/// # let profile = JsonCodec::decode(include_bytes!(
/// #   concat!(
/// #     "../../../",
/// #     "tests/data/cache/",
/// #     "windows/ntkrnlmp.pdb/ce7ffb00c20b87500211456b3e905c471/profile.json"
/// #   )
/// # ))?;
/// let offsets = Offsets::new(&profile)?;
///
/// let refcnt = offsets._EX_FAST_REF.RefCnt.value_from(0x1234567890abcdef);
/// assert_eq!(offsets._EX_FAST_REF.RefCnt.bit_position, 0);
/// assert_eq!(offsets._EX_FAST_REF.RefCnt.bit_length, 4);
/// assert_eq!(refcnt, 0xf);
///
/// assert!(!offsets._EPROCESS.is_empty());
/// assert_eq!(offsets._EPROCESS.len(), 2176);
///
/// // The field with the largest offset + size in the `Offset` struct
/// // is `WoW64Process` (offset 1064, size 8), so the effective length
/// // of the structure is 1072 bytes.
/// assert_eq!(offsets._EPROCESS.effective_len(), 1072);
///
/// assert_eq!(offsets._EPROCESS.UniqueProcessId.offset, 744);
/// assert_eq!(offsets._EPROCESS.UniqueProcessId.size, 8);
///
/// assert_eq!(offsets._EPROCESS.WoW64Process.offset, 1064);
/// assert_eq!(offsets._EPROCESS.WoW64Process.size, 8);
///
/// assert_eq!(offsets._EPROCESS.Affinity.offset, 80);
/// assert_eq!(offsets._EPROCESS.Affinity.size, 168);
/// # Ok(())
/// # }
/// ```
///
/// # Attributes
///
/// - `#[isr(alias = <alias>)]`: Specifies an alternative name for a field or
///   structure. This is useful if a field might have a different name across
///   OS builds or kernel versions.
///
///   `<alias>` can be a single literal or an array of literals, e.g.:
///   - `#[isr(alias = "alternative_name")]`
///   - `#[isr(alias = ["name1", "name2", ...])]`
///
/// The generated struct provides a `new` method that takes a reference to
/// a [`Profile`] and returns a [`Result`] containing the populated struct or
/// an error if any fields or structures are not found.
///
/// Each inner struct also implements the following convenience methods:
/// - `is_empty()`: Returns `true` if the structure has zero size.
/// - `len()`: Returns the size of the structure in bytes.
/// - `effective_len()`: Returns the offset of the last defined field plus its size.
///
/// [`Profile`]: isr_core::Profile
#[macro_export]
macro_rules! offsets {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $($rest:tt)*
        }
    ) => {
        $crate::offsets!(@outer
            $vis,
            [ $(#[$meta])* ],
            struct $name {
                $($rest)*
            }
        );

        $crate::offsets!(@inner
            $vis,
            [ $(#[$meta])* ],
            $($rest)*
        );
    };

    (@outer
        $vis:vis,
        [$($meta:tt)*],
        struct $name:ident {
            $(
                $(#[isr($($iattr:tt)*)])?
                struct $iname:ident {
                    $(
                        $(#[isr($($fattr:tt)*)])?
                        $fname:ident: $ftype:ty
                    ),* $(,)?
                }
            )+
        }
    ) => {
        #[allow(non_camel_case_types, non_snake_case, missing_docs)]
        $($meta)*
        $vis struct $name {
            $(
                $vis $iname: $iname,
            )*
        }

        impl $name {
            /// Creates a new offsets instance.
            $vis fn new(profile: &$crate::__private::Profile) -> Result<Self, $crate::Error> {
                Ok(Self {
                    $(
                        $iname: $iname::new(profile)?,
                    )+
                })
            }
        }
    };

    (@inner
        $vis:vis,
        [$($meta:tt)*],
        $(#[isr($($iattr:tt)*)])?
        struct $iname:ident {
            $(
                $(#[isr($($fattr:tt)*)])?
                $fname:ident: $ftype:ty
            ),* $(,)?
        }

        $($rest:tt)*
    ) => {
        #[allow(non_camel_case_types, non_snake_case, missing_docs)]
        $($meta)*
        $vis struct $iname {
            $(
                pub $fname: $ftype,
            )*
            __len: usize,
            __effective_len: usize,
        }

        impl $iname {
            #[doc = concat!("Creates a new `", stringify!($iname), "` instance.")]
            $vis fn new(profile: &$crate::__private::Profile) -> Result<Self, $crate::Error> {
                use $crate::__private::IntoField as _;

                let name = $crate::offsets!(@find
                    profile,
                    $iname,
                    [$($($iattr)*)?]
                ).ok_or($crate::Error::type_not_found(stringify!($iname)))?;

                let len = profile
                    .struct_size(name)
                    .ok_or($crate::Error::type_not_found(name))?;
                let mut effective_len: u64 = 0;

                $(
                    effective_len = u64::max(
                        effective_len,
                        match $crate::offsets!(@assign
                            profile,
                            name,
                            $fname,
                            [$($($fattr)*)?]
                        ) {
                            Ok(descriptor) => descriptor.size() + descriptor.offset(),
                            Err(_) => 0,
                        }
                    );
                )*

                Ok(Self {
                    $(
                        $fname: $crate::offsets!(@assign
                            profile,
                            name,
                            $fname,
                            [$($($fattr)*)?]
                        ).into_field()?,
                    )*
                    __len: len as usize,
                    __effective_len: effective_len as usize,
                })
            }

            /// Returns `true` if the structure does not contain any fields.
            $vis fn is_empty(&self) -> bool {
                self.__len == 0
            }

            /// Returns the size of the structure in bytes.
            $vis fn len(&self) -> usize {
                self.__len
            }

            /// Returns the effective size of the structure in bytes.
            ///
            /// The effective size is the offset of the last defined field plus its size.
            $vis fn effective_len(&self) -> usize {
                self.__effective_len
            }
        }

        $crate::offsets!(@inner
            $vis,
            [$($meta)*],
            $($rest)*
        );
    };

    (@inner
        $vis:vis,
        [$($meta:tt)*],
    ) => {};

    //
    // @find
    //

    (@find
        $profile:ident,
        $iname:ident,
        []
    ) => {{
        $profile
            .find_struct(stringify!($iname))
            .map(|_| stringify!($iname))
    }};

    (@find
        $profile:ident,
        $iname:ident,
        [alias = $alias:literal]
    ) => {{
        $profile
            .find_struct(stringify!($iname))
            .map(|_| stringify!($iname))
            .or_else(|| $profile
                .find_struct($alias)
                .map(|_| $alias)
            )
    }};

    (@find
        $profile:ident,
        $iname:ident,
        [alias = [$($alias:literal),+ $(,)?]]
    ) => {{
        $profile
            .find_struct(stringify!($iname))
            .map(|_| stringify!($iname))
            $(
                .or_else(|| $profile
                    .find_struct($alias)
                    .map(|_| $alias)
                )
            )+
    }};

    //
    // @assign
    //

    (@assign
        $profile:ident,
        $iname:ident,
        $fname:ident,
        []
    ) => {{
        use $crate::__private::ProfileExt as _;

        $profile
            .find_field_descriptor($iname, stringify!($fname))
    }};

    (@assign
        $profile:ident,
        $iname:ident,
        $fname:ident,
        [alias = $alias:literal]
    ) => {{
        use $crate::__private::ProfileExt as _;

        $profile
            .find_field_descriptor($iname, stringify!($fname))
            .or_else(|_| $profile
                .find_field_descriptor($iname, $alias)
            )
    }};

    (@assign
        $profile:ident,
        $iname:ident,
        $fname:ident,
        [alias = [$($alias:literal),+ $(,)?]]
    ) => {{
        use $crate::__private::ProfileExt as _;

        $profile
            .find_field_descriptor($iname, stringify!($fname))
            $(
                .or_else(|_| $profile
                    .find_field_descriptor($iname, $alias)
                )
            )+
    }};
}
