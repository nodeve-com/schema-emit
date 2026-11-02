//! What a check or a type carries as data: numbers, literals, notes.

/// A number in a schema, as written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Num {
    Int(i128),
    Float(f64),
}

/// A literal value.
#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Null,
    Bool(bool),
    Int(i128),
    Float(f64),
    Text(String),
    /// A list's items, or a tuple's positions in order.
    List(Vec<Lit>),
    /// A model's fields by name.
    Object(Vec<(String, Lit)>),
}

/// Annotation text a type carries beside its check, by keyword name, for the embedder: the
/// emitter never reads these.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notes(pub(crate) std::collections::BTreeMap<String, String>);

impl Notes {
    /// The text carried for `keyword`.
    pub fn get(&self, keyword: &str) -> Option<&str> {
        self.0.get(keyword).map(String::as_str)
    }
}
