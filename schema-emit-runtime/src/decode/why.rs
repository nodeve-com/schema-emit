//! [`Why`]: the reason a value has no std form.

use std::borrow::Cow;

/// Why a value has no std form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    /// The value is another kind than the type takes; the text says which kind it takes.
    Kind(Cow<'static, str>),
    /// A required field is absent.
    Absent,
    /// The value is the right kind but past what the type holds: an integer outside its width,
    /// or an instant or span outside chrono's range. The text says which.
    Range(Cow<'static, str>),
    /// A closed model has a key that no field names.
    Unnamed,
    /// The value is the right kind and no std value holds it for another reason, such as text
    /// the deserializer can't read.
    Other(Cow<'static, str>),
}

impl Why {
    /// The reason's text, without the `expected` a kind adds when printed.
    pub(super) fn text(&self) -> &str {
        match self {
            Why::Kind(t) | Why::Range(t) | Why::Other(t) => t,
            Why::Absent => "a required field is absent",
            Why::Unnamed => "a key no field names",
        }
    }
}
