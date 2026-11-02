//! The Rust names of a model's fields: each schema name as a snake-case identifier, made unique,
//! with the field holding the keys no field names last.

use crate::ir::Model;

/// Words that can't be a field name as written, and take `r#`.
const KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn",
    "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let",
    "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return",
    "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use",
    "virtual", "where", "while", "yield", "crate", "self", "super",
];

/// Words no `r#` can make a name: they take a trailing `_`.
const PATHS: &[&str] = &["crate", "self", "super"];

/// A model's field names, in order, and its field of other keys' name.
pub struct Names {
    pub fields: Vec<String>,
    pub others: String,
}

/// The Rust name of each of `names`, in order.
pub fn fields<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let names: Vec<&str> = names.into_iter().collect();
    let mut used: Vec<String> = Vec::new();
    let mut take = taker(&mut used);
    names.into_iter().map(&mut take).collect()
}

pub fn of(model: &Model) -> Names {
    let mut used: Vec<String> = Vec::new();
    let mut take = taker(&mut used);
    let fields = model.fields.iter().map(|f| take(&f.name)).collect();
    Names {
        fields,
        others: take("others"),
    }
}

/// Takes a name's identifier, unique among those `used`.
fn taker(used: &mut Vec<String>) -> impl FnMut(&str) -> String + '_ {
    |name: &str| {
        let mut id = snake(name);
        while used.contains(&id) {
            id.push('_');
        }
        used.push(id.clone());
        raw(id)
    }
}

/// `name` as words joined by `_`, an identifier's characters only.
pub(super) fn snake(name: &str) -> String {
    if name.is_empty() {
        return "value".into();
    }
    let mut out = String::new();
    let mut lower = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if c.is_ascii_uppercase() && lower {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
            lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        } else {
            if !out.ends_with('_') {
                out.push('_');
            }
            lower = false;
        }
    }
    if out.trim_matches('_').is_empty() {
        out.push_str("key");
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// `id` as a field name where it is a keyword.
fn raw(id: String) -> String {
    if PATHS.contains(&id.as_str()) {
        format!("{id}_")
    } else if KEYWORDS.contains(&id.as_str()) {
        format!("r#{id}")
    } else {
        id
    }
}
