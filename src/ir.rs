//! The model every front end lowers to and the emitter reads: models of typed fields, each type
//! a base and the checks on it. A base is what the embedder's type system already guarantees; a
//! check is what it can't, so the emitted code tests it.

/// Models, referred to by index: a front end's output.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Schema {
    pub models: Vec<Model>,
    /// What lowering holds but doesn't refuse, as a message: a mixed-kind tuple.
    pub warnings: Vec<String>,
}

/// An object of named fields, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub name: String,
    pub fields: Vec<Field>,
    /// Model-level checks on this model as the root: each count, dependency, or branch, as
    /// `Ty::checks` holds them for a model beside its field.
    pub checks: Vec<Check>,
    /// The type of the values under keys no field names, held where a check reads them. `None`
    /// where none does: the model is closed, or nothing judges those keys. `Base::Any` is for
    /// values no check reads.
    pub others: Option<Ty>,
    /// What a decode does with keys no field names and nothing reads.
    pub additional: Additional,
}

impl Model {
    /// Whether the model wraps a root that is no object: one field named empty holds the value,
    /// so a path to it is the model's own.
    pub fn is_value(&self) -> bool {
        matches!(self.fields.as_slice(), [f] if f.name.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
    /// Absent fails the check; else absent is silence.
    pub required: bool,
}

/// A type: its base, the checks on a value of it, and what it carries beside the check.
#[derive(Clone, Debug, PartialEq)]
pub struct Ty {
    pub base: Base,
    /// The value may be null beside its base. A null is no value of the base, so the checks
    /// hold of a non-null value only; a choice ([`Check::OneOf`]) admits null by its [`Lit::Null`],
    /// and a branch check by the branches that admit it.
    pub nullable: bool,
    pub checks: Vec<Check>,
    pub notes: Notes,
}

impl Ty {
    pub fn of(base: Base) -> Ty {
        Ty {
            base,
            nullable: false,
            checks: Vec::new(),
            notes: Notes::default(),
        }
    }
}

/// What the embedder's types guarantee of a value: its kind, never its range.
#[derive(Clone, Debug, PartialEq)]
pub enum Base {
    /// The only value is null.
    Null,
    Bool,
    /// Any value of any kind, as a boolean schema's. The embedder types it as it chooses, and
    /// only `Not` and an empty `OneOf` check it.
    Any,
    Int(Int),
    Float(Float),
    Text,
    /// Text of format `date-time`: an instant, which the embedder types as its timestamp.
    Timestamp,
    /// Text of format `human-duration`: an optional sign, then humantime's text such as `1.5s` or
    /// `90m`, which the embedder types as its duration.
    Duration,
    /// Text of `contentEncoding: base64`, the embedder's bytes.
    Bytes,
    List(Box<Ty>),
    /// An array of positions, each its own type; a position may be absent. `rest` is the item
    /// type of the items past the prefix, a list of its own, or `None` where the array is
    /// fixed-length.
    Tuple {
        prefix: Vec<Ty>,
        rest: Option<Box<Ty>>,
    },
    /// Text keys to values of one type.
    Map(Box<Ty>),
    /// [`Schema::models`]' index.
    Model(usize),
    /// One of several types, each a branch: the embedder's enum, a variant per branch in order.
    /// A value is the first branch it fits (`anyOf`), and its checks are that branch's.
    Union(Union),
}

/// A union's branches, in order, and the `$defs` key it is declared under, where it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Union {
    pub name: Option<String>,
    pub variants: Vec<Ty>,
}

impl Base {
    /// A value of this base is an array, an object, or a map, never a scalar.
    pub fn is_structured(&self) -> bool {
        matches!(
            self,
            Base::List(_) | Base::Tuple { .. } | Base::Map(_) | Base::Model(_) | Base::Union(_)
        )
    }

    /// Whether both are one kind of text: plain text, a timestamp, a duration, or bytes.
    pub fn same_text(&self, other: &Base) -> bool {
        self.json_kind() == Some(Json::Text) && self == other
    }

    /// The JSON kind a value of this base is. Values of two kinds are never equal; `None` for
    /// `Any`, which may be of every kind.
    pub fn json_kind(&self) -> Option<Json> {
        Some(match self {
            Base::Any => return None,
            Base::Null => Json::Null,
            Base::Bool => Json::Bool,
            Base::Int(_) | Base::Float(_) => Json::Number,
            Base::Text | Base::Timestamp | Base::Duration | Base::Bytes => Json::Text,
            Base::List(_) | Base::Tuple { .. } => Json::Array,
            Base::Map(_) | Base::Model(_) | Base::Union(_) => Json::Object,
        })
    }
}

/// A JSON value's kind, as [`Base::json_kind`] names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Json {
    Null,
    Bool,
    Number,
    Text,
    Array,
    Object,
}

/// An integer's width, named as Rust and cel-emit name it: the std form's type, and the bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Int {
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
}

impl Int {
    /// Its least and greatest value; a check reads an `i128`, so `U128`'s greatest is `i128::MAX`.
    pub fn bounds(self) -> (i128, i128) {
        match self {
            Int::I8 => (i8::MIN.into(), i8::MAX.into()),
            Int::I16 => (i16::MIN.into(), i16::MAX.into()),
            Int::I32 => (i32::MIN.into(), i32::MAX.into()),
            Int::I64 => (i64::MIN.into(), i64::MAX.into()),
            Int::I128 => (i128::MIN, i128::MAX),
            Int::U8 => (0, u8::MAX.into()),
            Int::U16 => (0, u16::MAX.into()),
            Int::U32 => (0, u32::MAX.into()),
            Int::U64 => (0, u64::MAX.into()),
            Int::U128 => (0, i128::MAX),
        }
    }

    pub fn signed(self) -> bool {
        matches!(self, Int::I8 | Int::I16 | Int::I32 | Int::I64 | Int::I128)
    }
}

/// A float's width: the check fails NaN, the infinities, and a value past the width's range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Float {
    F32,
    F64,
}

/// A constraint on a value, beyond its base. Each is one area's (crate docs).
#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    /// A number at least (or above, `exclusive`) `at`.
    Min { at: Num, exclusive: bool },
    /// A number at most (or below, `exclusive`) `at`.
    Max { at: Num, exclusive: bool },
    /// A number whose quotient by this is whole.
    MultipleOf(Num),
    /// A number with a zero fractional part: an integer's test on a float's view, so 1.0 passes.
    Whole,
    /// Text whose length is within these.
    Length {
        min: Option<u64>,
        max: Option<u64>,
        of: Count,
    },
    /// Text matching this pattern somewhere, as ECMA-262 reads it.
    Pattern(String),
    /// A value equal to one of these.
    OneOf(Vec<Lit>),
    /// A list of this many items.
    Items { min: Option<u64>, max: Option<u64> },
    /// A list, or a tuple with no trailing items, whose items are pairwise unequal, by JSON
    /// Schema's equality.
    Unique,
    /// A list with at least `min` and at most `max` items valid against `ty`, a scalar of the
    /// items' kind; each item's validity is a condition on its view.
    Contains {
        ty: Box<Ty>,
        min: u64,
        max: Option<u64>,
    },
    /// A model with this many keys present, or a map with this many entries.
    Properties { min: Option<u64>, max: Option<u64> },
    /// A model where, when key `when` is present, each of `then` is too. A key is a field's
    /// name, else one of [`Model::others`]; a name neither holds is never present.
    Dependent { when: String, then: Vec<String> },
    /// A model where each of these keys is present, named as in [`Check::Dependent`].
    Present(Vec<String>),
    /// A model whose other key matching `pattern` has a value valid against `ty`
    /// (`patternProperties`).
    Matching { pattern: String, ty: Box<Ty> },
    /// A model whose other key matching none of `patterns` has a value valid against `ty`
    /// (`additionalProperties`).
    Unmatched { patterns: Vec<String>, ty: Box<Ty> },
    /// A model whose every key present is text valid against `ty` (`propertyNames`).
    Names(Box<Ty>),
    /// A model where each field outside these names is absent: the fields its schema's scope
    /// leaves unevaluated (`unevaluatedProperties: false`).
    Unevaluated(Vec<String>),
    /// A value that fails when every check of this type holds on it: its base is the value's
    /// kind, or `Any`.
    Not(Box<Ty>),
    /// A value valid against exactly one of these (`oneOf`). Each is a branch: a type lowered
    /// onto the value's base, nullable where it admits null.
    ExactlyOne(Vec<Ty>),
    /// A value valid against at least one of these branches (`anyOf`).
    AtLeastOne(Vec<Ty>),
    /// A value valid against branch `then` where branch `when` is valid, and against
    /// `otherwise` where it isn't; an absent branch holds (`if`, `then`, `else`).
    Conditional {
        when: Box<Ty>,
        then: Option<Box<Ty>>,
        otherwise: Option<Box<Ty>>,
    },
    /// In a branch over a model: the value under other key `name` checked as `ty`; absent fails
    /// where `required`.
    Entry {
        name: String,
        ty: Box<Ty>,
        required: bool,
    },
    /// In a branch over a model: field `index` of the value's model checked as `ty`, its base
    /// the field's and its checks the branch's own; absent fails where `required`. A branch's
    /// model checks only the fields it names, so the value's own field checks run once.
    Field {
        index: usize,
        ty: Box<Ty>,
        required: bool,
    },
}

impl Check {
    /// Whether this judges a value by branches, each a type of its own.
    pub fn is_branch(&self) -> bool {
        matches!(
            self,
            Check::ExactlyOne(_) | Check::AtLeastOne(_) | Check::Conditional { .. }
        )
    }
}

/// What a text length counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Count {
    /// Unicode scalar values, as JSON Schema counts.
    Chars,
    /// UTF-8 bytes, as CDDL's `.size` counts.
    Bytes,
}

mod values;

pub use values::{Additional, Lit, Notes, Num};
