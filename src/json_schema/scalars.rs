//! Area: scalars and widths. `type` of `boolean`, `integer`, `number`, `string`, and a number's
//! width by `format`, OpenAPI's registry names: an integer without one is `int64`, a number
//! `double`. On text, and on a number or integer naming a text format, `format` is an annotation,
//! as 2020-12 reads it by default, except `date-time` and `human-duration`, and `contentEncoding:
//! base64`, which make a string a timestamp, a duration, or bytes. `format: duration` stays text,
//! asserted against RFC 3339 Appendix A's grammar.

use serde_json::{Map, Value};

use super::{Done, Key};
use crate::ir::{Base, Check, Float, Int};
use crate::Error;

/// The text formats 2020-12 names. Beside a number they are annotations, as beside text.
const TEXT_FORMATS: [&str; 19] = [
    "date-time",
    "date",
    "time",
    "duration",
    "email",
    "idn-email",
    "hostname",
    "idn-hostname",
    "ipv4",
    "ipv6",
    "uri",
    "uri-reference",
    "iri",
    "iri-reference",
    "uuid",
    "uri-template",
    "json-pointer",
    "relative-json-pointer",
    "regex",
];

/// RFC 3339 Appendix A's `duration`: `P`, then a date part, a time part, or weeks.
const DURATION: &str = concat!(
    "^P(?:",
    "(?:[0-9]+D|[0-9]+M(?:[0-9]+D)?|[0-9]+Y(?:[0-9]+M(?:[0-9]+D)?)?)",
    "(?:T(?:[0-9]+H(?:[0-9]+M(?:[0-9]+S)?)?|[0-9]+M(?:[0-9]+S)?|[0-9]+S))?",
    "|T(?:[0-9]+H(?:[0-9]+M(?:[0-9]+S)?)?|[0-9]+M(?:[0-9]+S)?|[0-9]+S)",
    "|[0-9]+W",
    ")$"
);

pub(super) fn base(t: &str, s: &Map<String, Value>) -> Result<Base, Error> {
    let format = s
        .get("format")
        .and_then(Value::as_str)
        .filter(|f| !TEXT_FORMATS.contains(f));
    Ok(match (t, format) {
        ("boolean", _) => Base::Bool,
        ("string", _) if s.get("contentEncoding").and_then(Value::as_str) == Some("base64") => {
            Base::Bytes
        }
        ("string", _) => match s.get("format").and_then(Value::as_str) {
            Some("date-time") => Base::Timestamp,
            Some("human-duration") => Base::Duration,
            _ => Base::Text,
        },
        ("integer", None) => Base::Int(Int::I64),
        ("integer", Some(f)) => Base::Int(match f {
            "int8" => Int::I8,
            "int16" => Int::I16,
            "int32" => Int::I32,
            "int64" => Int::I64,
            "uint8" => Int::U8,
            "uint16" => Int::U16,
            "uint32" => Int::U32,
            "uint64" => Int::U64,
            _ => return Err(Error::invalid(format!("integer format `{f}`"))),
        }),
        ("number", None | Some("double")) => Base::Float(Float::F64),
        ("number", Some("float")) => Base::Float(Float::F32),
        ("number", Some(f)) => return Err(Error::invalid(format!("number format `{f}`"))),
        ("null", _) => Base::Null,
        (t, _) => return Err(Error::invalid(format!("type `{t}`"))),
    })
}

pub(super) fn keyword(k: &mut Key) -> Done {
    match k.key {
        "type" => Some(Ok(())),
        "format" => {
            if k.value.as_str() == Some("duration") && k.ty.base == Base::Text {
                k.ty.checks.push(Check::Pattern(DURATION.into()));
            }
            Some(Ok(()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use serde_json::json;

    use crate::ir::Base;

    fn base(schema: serde_json::Value) -> Base {
        let lowered = crate::json_schema::lower("Value", &schema, &Default::default(), &[]);
        let (s, m) = lowered.unwrap();
        s.models[m].fields[0].ty.base.clone()
    }

    fn field(schema: serde_json::Value) -> Base {
        base(json!({ "type": "object", "properties": { "f": schema } }))
    }

    #[test]
    fn a_date_time_human_duration_or_base64_string_reads_as_its_type() {
        let date_time = json!({ "type": "string", "format": "date-time" });
        assert_eq!(field(date_time), Base::Timestamp);
        let duration = json!({ "type": "string", "format": "human-duration" });
        assert_eq!(field(duration), Base::Duration);
        let bytes = json!({ "type": "string", "contentEncoding": "base64" });
        assert_eq!(field(bytes), Base::Bytes);
    }

    #[test]
    fn a_duration_format_is_text_checked_by_a_pattern() {
        let duration = json!({ "type": "string", "format": "duration" });
        assert_eq!(field(duration), Base::Text);
    }

    #[test]
    fn other_text_formats_and_encodings_stay_text() {
        for s in [
            json!({ "type": "string", "format": "date" }),
            json!({ "type": "string", "format": "email" }),
            json!({ "type": "string", "contentEncoding": "base16" }),
            json!({ "type": "string" }),
        ] {
            assert_eq!(field(s), Base::Text);
        }
    }

    #[test]
    fn a_typed_string_keeps_its_type_through_a_reference() {
        let schema = json!({
            "$defs": { "At": { "type": "string", "format": "date-time" } },
            "type": "object",
            "properties": { "f": { "$ref": "#/$defs/At" } }
        });
        assert_eq!(base(schema), Base::Timestamp);
    }
}
