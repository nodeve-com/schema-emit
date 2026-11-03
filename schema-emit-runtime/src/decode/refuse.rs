//! `refuse!`: the `Visitor` methods of the kinds a visitor doesn't take. Each fails as
//! [`Unfit::kind`](super::Unfit::kind) of what the visitor expects, so a deserializer never
//! formats the failure itself, as serde's default for a kind is to do.

/// Writes, inside `impl Visitor`, a method per named kind that fails as the wrong kind, with
/// `$expected` as what the visitor takes. The kinds: `unit`, `none`, `some`, `bool`, `i64`,
/// `u64`, `f64`, `str`, `bytes`, `seq`, `map`, `enum`, `newtype`.
macro_rules! refuse {
    ($expected:expr; $($kind:ident)*) => { $( refuse!(@ $kind, $expected); )* };
    (@ unit, $e:expr) => {
        fn visit_unit<E: ::serde::de::Error>(self) -> Result<Self::Value, E> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
    (@ none, $e:expr) => {
        fn visit_none<E: ::serde::de::Error>(self) -> Result<Self::Value, E> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
    (@ some, $e:expr) => {
        fn visit_some<D: ::serde::de::Deserializer<'de>>(self, _: D) -> Result<Self::Value, D::Error> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
    (@ bool, $e:expr) => {
        fn visit_bool<E: ::serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
    (@ i64, $e:expr) => {
        fn visit_i64<E: ::serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
    (@ u64, $e:expr) => {
        fn visit_u64<E: ::serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
    (@ f64, $e:expr) => {
        fn visit_f64<E: ::serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
    (@ str, $e:expr) => {
        fn visit_str<E: ::serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
    (@ bytes, $e:expr) => {
        fn visit_bytes<E: ::serde::de::Error>(self, _: &[u8]) -> Result<Self::Value, E> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
    (@ seq, $e:expr) => {
        fn visit_seq<A: ::serde::de::SeqAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
    (@ map, $e:expr) => {
        fn visit_map<A: ::serde::de::MapAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
    (@ enum, $e:expr) => {
        fn visit_enum<A: ::serde::de::EnumAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
    (@ newtype, $e:expr) => {
        fn visit_newtype_struct<D: ::serde::de::Deserializer<'de>>(self, _: D) -> Result<Self::Value, D::Error> {
            Err($crate::decode::Unfit::kind($e).raise())
        }
    };
}
