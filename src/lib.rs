#![doc = include_str!("../README.md")]

#[cfg(feature = "json-schema")]
pub mod build;
#[cfg(feature = "json-schema")]
pub mod json_schema;

pub mod emit;
pub mod ir;
pub mod matching;
#[doc = include_str!("../docs/missing.md")]
pub mod missing {}

/// Why a schema doesn't lower or emit: its kind, and what, where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// Something no area handles yet, else a mistake in the schema.
    pub unsupported: bool,
    pub message: String,
}

impl Error {
    pub fn invalid(message: impl Into<String>) -> Error {
        Error {
            unsupported: false,
            message: message.into(),
        }
    }

    pub fn unsupported(message: impl Into<String>) -> Error {
        Error {
            unsupported: true,
            message: message.into(),
        }
    }

    /// The same error, saying where.
    pub fn at(self, place: &str) -> Error {
        let message = format!("{place}: {}", self.message);
        Error { message, ..self }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = if self.unsupported {
            "unsupported"
        } else {
            "invalid"
        };
        write!(f, "{kind}: {}", self.message)
    }
}
