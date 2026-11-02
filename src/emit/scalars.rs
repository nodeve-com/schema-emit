//! Area: scalars and widths. A number's view, and the condition its width puts on it: an integer
//! within its width's bounds, a float finite and within its width's range.

use crate::ir::{Base, Float, Int};

/// `let <view> = …;` for a bool or number at `value`, a `&` of it.
pub(super) fn view(base: &Base, value: &str, view: &str) -> String {
    match base {
        Base::Int(_) => format!("let {view}: i128 = (*{value}) as i128;"),
        Base::Float(_) => format!("let {view}: f64 = (*{value}) as f64;"),
        _ => format!("let {view} = *{value};"),
    }
}

/// What the base's width demands of `view`, if anything: an `i64` holds only what its width
/// allows, so its width demands nothing.
pub(super) fn width(base: &Base, view: &str) -> Option<String> {
    match base {
        Base::Int(Int::I64) => None,
        Base::Int(w) => Some(int(*w, view)),
        Base::Float(Float::F64) => Some(format!("{view}.is_finite()")),
        Base::Float(Float::F32) => Some(format!(
            "{view}.is_finite() && {view}.abs() <= {:?}",
            f32::MAX as f64
        )),
        _ => None,
    }
}

fn int(w: Int, view: &str) -> String {
    let (lo, hi) = w.bounds();
    format!("{view} >= {lo} && {view} <= {hi}")
}
