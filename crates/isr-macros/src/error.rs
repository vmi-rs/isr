#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to convert value: {0}")]
    Conversion(&'static str),

    #[error("Failed to find symbol {0}")]
    SymbolNotFound(String),

    #[error("Failed to find type {0}")]
    TypeNotFound(String),

    #[error("Failed to find field {field_name} in type {type_name}")]
    FieldNotFound {
        type_name: String,
        field_name: String,
    },
}

impl Error {
    pub fn symbol_not_found(symbol_name: impl Into<String>) -> Self {
        Self::SymbolNotFound(symbol_name.into())
    }

    pub fn type_not_found(type_name: impl Into<String>) -> Self {
        Self::TypeNotFound(type_name.into())
    }

    pub fn field_not_found(type_name: impl Into<String>, field_name: impl Into<String>) -> Self {
        Self::FieldNotFound {
            type_name: type_name.into(),
            field_name: field_name.into(),
        }
    }
}
