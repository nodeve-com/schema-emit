//! Counts the allocation calls and bytes between `measure`'s start and stop: the measured
//! program's global allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

static ON: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

pub struct Counting;

fn grow(by: usize) {
    CALLS.fetch_add(1, Relaxed);
    BYTES.fetch_add(by, Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        ON.load(Relaxed).then(|| grow(layout.size()));
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ON.load(Relaxed).then(|| grow(size));
        System.realloc(ptr, layout, size)
    }
}

/// `f`'s value, with the allocation calls and bytes it made.
pub fn measure<T>(f: impl FnOnce() -> T) -> (T, usize, usize) {
    CALLS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    ON.store(true, Relaxed);
    let out = framed(f);
    ON.store(false, Relaxed);
    (out, CALLS.load(Relaxed), BYTES.load(Relaxed))
}

/// `f`'s value, called from a stack frame of its own, never as a tail call, so callgrind ends a
/// function it entered once this frame returns.
#[inline(never)]
fn framed<T>(f: impl FnOnce() -> T) -> T {
    std::hint::black_box(f())
}
