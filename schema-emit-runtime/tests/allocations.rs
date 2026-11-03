//! How many allocations a decode makes: a list of records with text, a list, an optional field,
//! and a union, read the way emitted decode functions compose the readers.

#![cfg(feature = "decode")]
#![allow(clippy::unwrap_used)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use schema_emit_runtime::decode::{self as d, Unfit};

/// The system allocator, counting the calls this thread makes while counting is on.
struct Counting;

thread_local! {
    static CALLS: Cell<Option<usize>> = const { Cell::new(None) };
}

fn tally() {
    let _ = CALLS.try_with(|c| c.set(c.get().map(|n| n.saturating_add(1))));
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        tally();
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        tally();
        System.realloc(ptr, layout, size)
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// The allocation calls `f` makes on this thread.
fn calls<T>(f: impl FnOnce() -> T) -> (T, usize) {
    CALLS.with(|c| c.set(Some(0)));
    let out = f();
    (out, CALLS.with(|c| c.replace(None)).unwrap_or_default())
}

#[derive(Debug, PartialEq)]
struct Row {
    id: i64,
    name: String,
    tags: Vec<String>,
    note: Option<String>,
    cell: Cell2,
}

#[derive(Debug, PartialEq)]
enum Cell2 {
    Int(i64),
    Text(String),
}

fn read_cell(v: &str) -> Result<Cell2, Unfit> {
    let int = d::int(v).map(Cell2::Int);
    int.or_else(|e| {
        d::text(v)
            .map(Cell2::Text)
            .map_err(|f| Unfit::either(Some(e), f))
    })
}

fn read_row(v: &str) -> Result<Row, Unfit> {
    let [id, name, tags, note, cell] = d::object(v, &["id", "name", "tags", "note", "cell"])?;
    Ok(Row {
        id: d::field(id, "id", d::int)?,
        name: d::field(name, "name", d::text)?,
        tags: d::field(tags, "tags", |x| d::list(x, d::text))?,
        note: d::optional(note, "note", d::text)?,
        cell: d::field(cell, "cell", read_cell)?,
    })
}

const ROWS: &str = r#"[
  {"id": 1, "name": "one", "tags": ["a", "b"], "note": "first", "cell": 1.0},
  {"id": 2, "name": "two", "tags": [], "cell": "x", "unknown": [1, {"k": 2}]},
  {"id": 3, "name": "three", "tags": ["c"], "cell": 3}
]"#;

/// The rows' owned text and lists: names, tag lists, tags, notes, text cells, and the row list,
/// each one allocation.
const OWNED: usize = 3 + 2 + 3 + 1 + 1 + 1;

#[test]
fn a_decode_allocates_about_what_it_returns() {
    let (rows, n) = calls(|| d::root(ROWS, "Rows", |v| d::list(v, read_row)));
    let rows = rows.unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].cell, Cell2::Int(1));
    assert_eq!(rows[1].cell, Cell2::Text("x".into()));
    // Beside what it returns: a list's growth, and the reader's stack of brackets it skips.
    assert!(n <= OWNED + 4, "{n} allocation calls");
}
