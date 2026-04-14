/// Errors returned by `symbols!` / `offsets!` macro-generated accessors.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Field descriptor shape did not match the target type.
    #[error("descriptor mismatch: {0}")]
    DescriptorMismatch(&'static str),

    /// No symbol with the given name was found in the profile.
    #[error("symbol `{0}` not found")]
    SymbolNotFound(String),

    /// No type with the given name was found in the profile.
    #[error("type `{0}` not found")]
    TypeNotFound(String),

    /// No field with the given name was found in the named type, even after
    /// recursing into nested struct members.
    #[error("field `{field_name}` not found in type `{type_name}`")]
    FieldNotFound {
        /// Name of the struct that was searched.
        type_name: String,
        /// Name of the field being looked up.
        field_name: String,
    },
}

impl Error {
    /// Constructs a [`Error::SymbolNotFound`] with the given symbol name.
    pub fn symbol_not_found(symbol_name: impl Into<String>) -> Self {
        Self::SymbolNotFound(symbol_name.into())
    }

    /// Constructs a [`Error::TypeNotFound`] with the given type name.
    pub fn type_not_found(type_name: impl Into<String>) -> Self {
        Self::TypeNotFound(type_name.into())
    }

    /// Constructs a [`Error::FieldNotFound`] with the given type and field names.
    pub fn field_not_found(type_name: impl Into<String>, field_name: impl Into<String>) -> Self {
        Self::FieldNotFound {
            type_name: type_name.into(),
            field_name: field_name.into(),
        }
    }
}
