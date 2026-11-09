//! A number's width: the std form's type, and the range a check holds it to.

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
