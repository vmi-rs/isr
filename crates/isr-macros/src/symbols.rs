use crate::Error;

/// A symbol descriptor.
#[derive(Debug, Clone)]
pub struct SymbolDescriptor {
    /// The virtual address offset of the symbol.
    pub offset: u64,
}

impl TryFrom<SymbolDescriptor> for u64 {
    type Error = Error;

    fn try_from(value: SymbolDescriptor) -> Result<Self, Self::Error> {
        Ok(value.offset)
    }
}

//
//
//

pub trait IntoSymbol<T> {
    type Error;

    fn into_symbol(self) -> Result<T, Error>;
}

impl IntoSymbol<u64> for Result<SymbolDescriptor, Error> {
    type Error = Error;

    fn into_symbol(self) -> Result<u64, Error> {
        self?.try_into()
    }
}

impl IntoSymbol<Option<u64>> for Result<SymbolDescriptor, Error> {
    type Error = Error;

    fn into_symbol(self) -> Result<Option<u64>, Error> {
        match self {
            Ok(symbol) => Ok(Some(symbol.try_into()?)),
            Err(_) => Ok(None),
        }
    }
}

/// Defines a set of symbols.
///
/// This macro simplifies the process of defining symbols for later use
/// with the `isr` crate, enabling type-safe access to symbol addresses
/// and offsets. It generates a struct with fields corresponding to
/// the defined symbols.
///
/// # Usage
///
/// ```rust
/// # use isr::{
/// #     cache::{Codec as _, JsonCodec},
/// #     macros::symbols,
/// # };
/// #
/// symbols! {
///     #[derive(Debug)]
///     pub struct Symbols {
///         PsActiveProcessHead: u64,
///
///         // Optional symbols might be missing from profile.
///         PsInitialSystemProcess: Option<u64>,
///         NonExistentSymbol: Option<u64>,
///
///         // Provide aliases when symbols might have different names across builds.
///         #[isr(alias = "KiSystemCall64Shadow")]
///         KiSystemCall64: u64,
///
///         // Multiple aliases for a symbol.
///         #[isr(alias = ["_NtOpenFile@24", "NtOpenFile"])]
///         NtOpenFile: u64, // Address of the NtOpenFile function
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
/// let symbols = Symbols::new(&profile)?;
/// assert_eq!(symbols.PsActiveProcessHead, 0x437BC0);
/// assert_eq!(symbols.PsInitialSystemProcess, Some(0x5733A0));
/// assert_eq!(symbols.NonExistentSymbol, None);
/// # Ok(())
/// # }
/// ```
///
/// # Attributes
///
/// - `#[isr(alias = <alias>)]`: Specifies an alternative name for the symbol.
///   This is useful when the symbol has different names across different OS
///   builds or versions.
///
/// - `#[isr(override = <override>)]`: Overrides the symbol name with a custom
///   name. This is useful when the symbol name should be different from the
///   field name.
///
///   `<alias>` and `<override>` can be a single literal or an array
///   of literals, e.g.:
///   - `#[isr(alias = "alternative_name")]`
///   - `#[isr(alias = ["name1", "name2", ...])]`
///
/// The generated struct provides a `new` method that takes a reference to
/// a [`Profile`] and returns a `Result` containing the populated struct or
/// an error if any symbols are not found.
///
/// [`Profile`]: isr_core::Profile
#[macro_export]
macro_rules! symbols {
    (
        $(#[$symbols_attrs:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[isr($($isr_attr:tt)*)])?
                $fname:ident: $ftype:ty
            ),+ $(,)?
        }
    ) => {
        $(#[$symbols_attrs])*
        #[allow(non_camel_case_types, non_snake_case, missing_docs)]
        $vis struct $name {
            $($vis $fname: $ftype),+
        }

        impl $name {
            /// Creates a new symbol instance.
            $vis fn new(profile: &$crate::__private::Profile) -> Result<Self, $crate::Error> {
                use $crate::__private::IntoSymbol as _;

                Ok(Self {
                    $(
                        $fname: $crate::symbols!(@assign
                            profile,
                            $fname,
                            [$($($isr_attr)*)?]
                        ).into_symbol()?,
                    )*
                })
            }
        }
    };

    (@assign
        $profile:ident,
        $fname:ident,
        []
    ) => {{
        use $crate::__private::ProfileExt as _;

        $profile
            .find_symbol_descriptor(stringify!($fname))
    }};

    (@assign
        $profile:ident,
        $fname:ident,
        [alias = $alias:literal]
    ) => {{
        use $crate::__private::ProfileExt as _;

        $profile
            .find_symbol_descriptor(stringify!($fname))
            .or_else(|_| $profile
                .find_symbol_descriptor($alias)
            )
    }};

    (@assign
        $profile:ident,
        $fname:ident,
        [alias = [$($alias:literal),+ $(,)?]]
    ) => {{
        use $crate::__private::ProfileExt as _;

        $profile
            .find_symbol_descriptor(stringify!($fname))
            $(
                .or_else(|_| $profile
                    .find_symbol_descriptor($alias)
                )
            )+
    }};

    (@assign
        $profile:ident,
        $fname:ident,
        [override = $override:literal]
    ) => {{
        use $crate::__private::ProfileExt as _;

        $profile
            .find_symbol_descriptor($override)
    }};

    (@assign
        $profile:ident,
        $fname:ident,
        [override = [$($override:literal),+ $(,)?]]
    ) => {{
        use $crate::__private::ProfileExt as _;

        Err($crate::Error::symbol_not_found(stringify!($fname)))
            $(
                .or_else(|_| $profile
                    .find_symbol_descriptor($override)
                )
            )+
    }};
}
