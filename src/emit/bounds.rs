//! Area: ranges and bounds. `Min`, `Max`, `MultipleOf`, and `Whole` on a number's view. An
//! integer's bound is an integer by the time it's written: a fractional one rounds inward, so the
//! comparison stays exact past 2^53.

use crate::ir::{Base, Check, Num};
use crate::Error;

pub(super) fn cond(check: &Check, base: &Base, view: &str) -> Option<Result<String, Error>> {
    let int = matches!(base, Base::Int(_));
    let float = matches!(base, Base::Float(_));
    if !int && !float {
        return match check {
            Check::Min { .. } | Check::Max { .. } | Check::MultipleOf(_) | Check::Whole => {
                Some(Err(Error::invalid(format!("{check:?} on {base:?}"))))
            }
            _ => None,
        };
    }
    Some(match check {
        Check::Whole if int => Ok("true".into()),
        // NaN and the infinities have no zero fractional part, so the width's test isn't needed.
        Check::Whole => Ok(format!("{view}.fract() == 0.0")),
        Check::Min { at, exclusive } if int => Ok(int_min(*at, *exclusive, view)),
        Check::Max { at, exclusive } if int => Ok(int_max(*at, *exclusive, view)),
        Check::Min { at, exclusive } => {
            Ok(float_cmp(view, if *exclusive { ">" } else { ">=" }, *at))
        }
        Check::Max { at, exclusive } => {
            Ok(float_cmp(view, if *exclusive { "<" } else { "<=" }, *at))
        }
        Check::MultipleOf(Num::Int(m)) if int && *m > 0 => Ok(format!("{view} % {m} == 0")),
        Check::MultipleOf(m) => match as_f64(*m) {
            m if m > 0.0 && m.is_finite() => Ok(format!(
                "{}::multiple_of({view} as f64, {})",
                super::STD,
                lit(m)
            )),
            _ => Err(Error::invalid(format!("multipleOf {m:?}"))),
        },
        _ => return None,
    })
}

/// An integer view at least `at`.
fn int_min(at: Num, exclusive: bool, view: &str) -> String {
    let least = match at {
        Num::Int(n) => Some(if exclusive { n.saturating_add(1) } else { n }),
        Num::Float(f) => whole(if exclusive && f == f.floor() {
            f + 1.0
        } else {
            f.ceil()
        }),
    };
    match least {
        Some(n) => format!("{view} >= {n}"),
        None if as_f64(at) < 0.0 => "true".into(),
        None => "false".into(),
    }
}

/// An integer view at most `at`.
fn int_max(at: Num, exclusive: bool, view: &str) -> String {
    let most = match at {
        Num::Int(n) => Some(if exclusive { n.saturating_sub(1) } else { n }),
        Num::Float(f) => whole(if exclusive && f == f.floor() {
            f - 1.0
        } else {
            f.floor()
        }),
    };
    match most {
        Some(n) => format!("{view} <= {n}"),
        None if as_f64(at) > 0.0 => "true".into(),
        None => "false".into(),
    }
}

/// `f`, whole, as an `i128` where it fits; NaN and the infinities never do.
fn whole(f: f64) -> Option<i128> {
    const LIMIT: f64 = 1.7e38;
    if f.is_finite() && f.abs() < LIMIT {
        #[allow(clippy::cast_possible_truncation)]
        Some(f as i128)
    } else {
        None
    }
}

fn as_f64(n: Num) -> f64 {
    match n {
        Num::Int(i) => i as f64,
        Num::Float(f) => f,
    }
}

fn float_cmp(view: &str, op: &str, at: Num) -> String {
    format!("{view} {op} {}", lit(as_f64(at)))
}

/// `f` as a Rust literal of its exact bits.
fn lit(f: f64) -> String {
    format!("f64::from_bits({:#x})", f.to_bits())
}
