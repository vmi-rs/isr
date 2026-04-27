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
    pub(crate) offset: u64,

    /// The size of the field, in bytes.
    pub(crate) size: u64,
}

impl Field {
    /// Creates a new field descriptor.
    pub fn new(offset: u64, size: u64) -> Self {
        Self { offset, size }
    }

    /// Returns the offset of the field from the beginning of the structure,
    /// in bytes.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Returns the size of the field, in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }
}

/// A bitfield within a structure.
///
/// `Bitfield` provides information about the offset, size, bit position, and
/// bit length of a bitfield member. It extends the functionality of [`Field`]
/// by allowing access to individual bits within a field.
#[derive(Debug, Clone, Copy)]
pub struct Bitfield {
    pub(crate) field: Field,

    /// The starting bit position of the bitfield within the underlying field.
    pub(crate) bit_position: u64,

    /// The length of the bitfield, in bits.
    pub(crate) bit_length: u64,
}

impl std::ops::Deref for Bitfield {
    type Target = Field;

    fn deref(&self) -> &Self::Target {
        &self.field
    }
}

impl Bitfield {
    /// Creates a new bitfield descriptor.
    pub fn new(offset: u64, size: u64, bit_position: u64, bit_length: u64) -> Self {
        Self {
            field: Field::new(offset, size),
            bit_position,
            bit_length,
        }
    }

    /// Returns the starting bit position of the bitfield within the underlying field.
    pub fn bit_position(&self) -> u64 {
        self.bit_position
    }

    /// Returns the length of the bitfield, in bits.
    pub fn bit_length(&self) -> u64 {
        self.bit_length
    }

    /// This method performs bitwise operations to isolate and return the
    /// value represented by the bitfield within the provided integer.
    pub fn extract(&self, value: u64) -> u64 {
        assert!(self.bit_length <= 64, "bit length cannot exceed 64 bits");
        assert!(
            self.bit_position + self.bit_length <= self.size * 8,
            "bitfield exceeds field size"
        );

        let result = value >> self.bit_position;
        let result = result & ((1 << self.bit_length) - 1);

        #[allow(clippy::let_and_return)]
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
                Err(Error::DescriptorMismatch("expected field, found bitfield"))
            }
        }
    }
}

impl TryFrom<FieldDescriptor> for Bitfield {
    type Error = Error;

    fn try_from(value: FieldDescriptor) -> Result<Self, Self::Error> {
        match value {
            // Allow converting a regular field to a bitfield with
            // bit position 0 and bit length equal to the field size in bits.
            FieldDescriptor::Field(field) => {
                if field.size == 0 {
                    return Err(Error::DescriptorMismatch(
                        "cannot convert zero-sized field to bitfield",
                    ));
                }

                if field.size > 8 {
                    return Err(Error::DescriptorMismatch(
                        "cannot convert field larger than 8 bytes to bitfield",
                    ));
                }

                Ok(Bitfield {
                    field,
                    bit_position: 0,
                    bit_length: field.size * 8,
                })
            }
            FieldDescriptor::Bitfield(bitfield) => Ok(bitfield),
        }
    }
}

//
//
//

/// Converts a field-descriptor lookup result into a concrete target type.
///
/// Implemented for the `Result<FieldDescriptor, Error>` return shape used by
/// code generated from the [`offsets!`] macro, with target types including
/// `Field`, `Bitfield`, and their `Option`-wrapped variants.
///
/// [`offsets!`]: crate::offsets
pub trait IntoField<T> {
    /// Conversion error type (currently always [`Error`]).
    type Error;

    /// Converts the lookup result into the target type, propagating any error.
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
/// # use isr_macros::{offsets, Bitfield, Field};
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
///
///         // Mark a structure as optional.
///         #[isr(optional)]
///         struct _LIST_ENTRY {
///             Flink: Field,
///             Blink: Field,
///         }
///
///         // Also optional, but the profile does not declare this
///         // type and the outer field will resolve to `None`.
///         #[isr(optional)]
///         struct _KMISSING_FROM_PROFILE {
///             Whatever: Field,
///         }
///     }
/// }
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Profile of a Windows 10.0.18362.356 kernel (synthetic fixture).
/// # let profile = isr_macros::__private::ntkrnlmp_profile();
/// let offsets = Offsets::new(&profile)?;
///
/// let refcnt = offsets._EX_FAST_REF.RefCnt.extract(0x1234567890abcdef);
/// assert_eq!(offsets._EX_FAST_REF.RefCnt.bit_position(), 0);
/// assert_eq!(offsets._EX_FAST_REF.RefCnt.bit_length(), 4);
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
/// assert_eq!(offsets._EPROCESS.UniqueProcessId.offset(), 744);
/// assert_eq!(offsets._EPROCESS.UniqueProcessId.size(), 8);
///
/// assert_eq!(offsets._EPROCESS.WoW64Process.offset(), 1064);
/// assert_eq!(offsets._EPROCESS.WoW64Process.size(), 8);
///
/// assert_eq!(offsets._EPROCESS.Affinity.offset(), 80);
/// assert_eq!(offsets._EPROCESS.Affinity.size(), 168);
///
/// // `_LIST_ENTRY` is present in the profile, so the optional field
/// // resolves to `Some(...)`.
/// let list_entry = offsets._LIST_ENTRY.as_ref().expect("_LIST_ENTRY present");
/// assert_eq!(list_entry.Flink.offset(), 0);
/// assert_eq!(list_entry.Blink.offset(), 8);
///
/// // `_KMISSING_FROM_PROFILE` is not declared, so it resolves to `None`.
/// assert!(offsets._KMISSING_FROM_PROFILE.is_none());
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
/// - `#[isr(optional)]`: Marks a structure as optional. The outer field
///   becomes `Option<...>` and resolves to `None` when no struct with the
///   given name (or any alias) is present in the profile. Missing fields
///   on a present type still error - `optional` only relaxes the existence
///   check on the type itself. Composes with `alias` in either order.
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
                $vis $iname: $crate::offsets!(@field-type $iname, [$($($iattr)*)?]),
            )*
        }

        impl $name {
            /// Creates a new offsets instance.
            $vis fn new(profile: &$crate::__private::Profile) -> Result<Self, $crate::Error> {
                Ok(Self {
                    $(
                        $iname: $crate::offsets!(@field-init profile, $iname, [$($($iattr)*)?]),
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

            /// Like [`Self::new`], but returns `Ok(None)` when the type is
            /// not present in the profile under any of its known names.
            $vis fn try_new(
                profile: &$crate::__private::Profile,
            ) -> Result<Option<Self>, $crate::Error> {
                match Self::new(profile) {
                    Ok(instance) => Ok(Some(instance)),
                    Err($crate::Error::TypeNotFound(_)) => Ok(None),
                    Err(err) => Err(err),
                }
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
    // The first three arms strip a struct-level `optional` token from
    // anywhere in the attribute list and re-dispatch to the alias arms
    // below.
    //

    (@find
        $profile:ident,
        $iname:ident,
        [optional]
    ) => {
        $crate::offsets!(@find $profile, $iname, [])
    };

    (@find
        $profile:ident,
        $iname:ident,
        [optional, $($rest:tt)*]
    ) => {
        $crate::offsets!(@find $profile, $iname, [$($rest)*])
    };

    (@find
        $profile:ident,
        $iname:ident,
        [$($head:tt)+, optional]
    ) => {
        $crate::offsets!(@find $profile, $iname, [$($head)+])
    };

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

    //
    // @field-type
    //
    // Walks the per-struct attribute list looking for `optional`. If found,
    // expands to `Option<$iname>`, otherwise expands to bare `$iname`.
    //

    (@field-type
        $iname:ident,
        [optional $(, $($_rest:tt)*)?]
    ) => {
        Option<$iname>
    };

    (@field-type
        $iname:ident,
        [$_head:tt $($rest:tt)*]
    ) => {
        $crate::offsets!(@field-type $iname, [$($rest)*])
    };

    (@field-type
        $iname:ident,
        []
    ) => {
        $iname
    };

    //
    // @field-init
    //
    // Same muncher shape as @field-type. Picks `try_new` when `optional` is
    // present anywhere in the attribute list, `new` otherwise.
    //

    (@field-init
        $profile:ident,
        $iname:ident,
        [optional $(, $($_rest:tt)*)?]
    ) => {
        $iname::try_new($profile)?
    };

    (@field-init
        $profile:ident,
        $iname:ident,
        [$_head:tt $($rest:tt)*]
    ) => {
        $crate::offsets!(@field-init $profile, $iname, [$($rest)*])
    };

    (@field-init
        $profile:ident,
        $iname:ident,
        []
    ) => {
        $iname::new($profile)?
    };
}
