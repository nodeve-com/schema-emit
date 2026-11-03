//! The [`Kinds`] of the numbers: a whole number of each width and a float of each width. Each
//! has a constructor returning its [`Shape`], which emitted code passes as the seed of a field.

use std::marker::PhantomData;

use serde::de::{self, Deserialize, DeserializeOwned, Deserializer};

use super::{Kinds, Raw, Shape, Unfit};

/// A whole number that an integer type holds.
pub trait Whole: Copy + DeserializeOwned {
    /// Why a number the type doesn't hold fails.
    const PAST: &'static str;
    /// Whether the type holds numbers a float can't, so the number's own text is read.
    const WIDE: bool;
    fn fit(v: i128) -> Option<Self>;
    fn fit_huge(v: u128) -> Option<Self>;
}

macro_rules! whole {
    ($($wide:literal: $($t:ty),*;)*) => {$($(
        impl Whole for $t {
            const WIDE: bool = $wide;
            const PAST: &'static str = concat!("an integer outside ", stringify!($t));
            fn fit(v: i128) -> Option<$t> {
                <$t>::try_from(v).ok()
            }
            fn fit_huge(v: u128) -> Option<$t> {
                <$t>::try_from(v).ok()
            }
        }
    )*)*};
}

whole!(false: i8, i16, i32, i64, u8, u16, u32, u64; true: i128, u128;);

/// An integer of type `T`. A float with no fraction, such as `1.0`, is one too.
pub struct Int<T>(PhantomData<fn() -> T>);

impl<T> Clone for Int<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Int<T> {}

impl<'de, T: Whole> Kinds<'de> for Int<T> {
    type Value = T;
    const EXPECTED: &'static str = "an integer";

    fn read<D: Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
        match T::WIDE {
            true => self.number(Raw::deserialize(d)?.get()),
            false => d.deserialize_any(Shape(self)),
        }
    }

    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
        T::deserialize(d)
    }

    fn int<E: de::Error>(self, v: i128) -> Result<T, E> {
        T::fit(v).ok_or_else(|| Unfit::range(T::PAST).raise())
    }

    fn huge<E: de::Error>(self, v: u128) -> Result<T, E> {
        T::fit_huge(v).ok_or_else(|| Unfit::range(T::PAST).raise())
    }

    fn float<E: de::Error>(self, f: f64) -> Result<T, E> {
        if f != f.trunc() {
            return Err(Self::wrong());
        }
        // `i128` holds every whole float below this.
        match f.abs() < 1.7e38 {
            #[allow(clippy::cast_possible_truncation)]
            true => self.int(f as i128),
            false => Err(Unfit::range(T::PAST).raise()),
        }
    }
}

impl<T: Whole> Int<T> {
    /// The number written as `text`, exact past what a float holds.
    fn number<E: de::Error>(self, text: &str) -> Result<T, E> {
        if let Ok(v) = text.parse::<i128>() {
            return self.int(v);
        }
        if let Ok(v) = text.parse::<u128>() {
            return self.huge(v);
        }
        match text.parse::<f64>() {
            Ok(f) if text.starts_with(|c: char| c == '-' || c.is_ascii_digit()) => self.float(f),
            _ => Err(Self::wrong()),
        }
    }
}

/// A float type.
pub trait Real: Copy + DeserializeOwned {
    fn make(f: f64) -> Self;
}

impl Real for f64 {
    fn make(f: f64) -> f64 {
        f
    }
}

impl Real for f32 {
    #[allow(clippy::cast_possible_truncation)]
    fn make(f: f64) -> f32 {
        f as f32
    }
}

/// A number, as a float of type `T`.
pub struct Float<T>(PhantomData<fn() -> T>);

impl<T> Clone for Float<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Float<T> {}

impl<'de, T: Real> Kinds<'de> for Float<T> {
    type Value = T;
    const EXPECTED: &'static str = "a number";

    fn binary<D: Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
        T::deserialize(d)
    }

    #[allow(clippy::cast_precision_loss)]
    fn int<E: de::Error>(self, v: i128) -> Result<T, E> {
        Ok(T::make(v as f64))
    }

    fn float<E: de::Error>(self, v: f64) -> Result<T, E> {
        Ok(T::make(v))
    }
}

pub fn int<T: Whole>() -> Shape<Int<T>> {
    Shape(Int(PhantomData))
}

pub fn float<T: Real>() -> Shape<Float<T>> {
    Shape(Float(PhantomData))
}
