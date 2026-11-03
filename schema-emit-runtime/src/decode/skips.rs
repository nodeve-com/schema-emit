//! [`Skips`]: the optional values a decode left out because they didn't fit. A decode that
//! collects ([`root`](super::root)) holds them in a slot of the thread; a `Deserialize` read
//! directly has no slot and notes nothing. Each part that returns leads the skips noted while it
//! was read with its own step, so a skip names its full place.

use std::cell::Cell;
use std::mem;
use std::thread_local;
use std::vec::Vec;

use super::Unfit;

/// How many skips are kept. The rest are counted.
const CAP: usize = 15;

/// The skips of a decode: the first ones, and how many came after them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Skips {
    pub kept: Vec<Unfit>,
    pub dropped: usize,
}

impl Skips {
    /// Whether the decode skipped nothing.
    pub fn is_empty(&self) -> bool {
        self.kept.is_empty() && self.dropped == 0
    }

    /// How many values the decode skipped.
    pub fn total(&self) -> usize {
        self.kept.len().saturating_add(self.dropped)
    }

    fn note(&mut self, unfit: Unfit) {
        match self.kept.len() < CAP {
            true => self.kept.push(unfit),
            false => self.dropped = self.dropped.saturating_add(1),
        }
    }
}

thread_local! {
    static SLOT: Cell<Option<Skips>> = const { Cell::new(None) };
}

/// Replaces the slot's skips, which are lost where the slot is gone (a thread ending).
fn exchange(next: Option<Skips>) -> Option<Skips> {
    SLOT.try_with(|slot| slot.replace(next)).ok().flatten()
}

/// What `f` gives of the skips being collected, none where nothing collects.
fn with<T>(f: impl FnOnce(&mut Skips) -> T) -> Option<T> {
    let mut skips = exchange(None)?;
    let out = f(&mut skips);
    exchange(Some(skips));
    Some(out)
}

/// Notes a skipped value, where a decode collects.
pub(super) fn note(unfit: Unfit) {
    with(|s| s.note(unfit));
}

/// Where the skips stand: the point a part began at.
#[derive(Clone, Copy)]
pub(super) struct Mark {
    kept: usize,
    dropped: usize,
}

pub(super) fn mark() -> Mark {
    with(|s| Mark {
        kept: s.kept.len(),
        dropped: s.dropped,
    })
    .unwrap_or(Mark {
        kept: 0,
        dropped: 0,
    })
}

/// Leads each skip noted since `mark` by `step`.
pub(super) fn lead(mark: Mark, step: impl Fn(Unfit) -> Unfit) {
    with(|s| {
        for kept in s.kept.iter_mut().skip(mark.kept) {
            let unfit = mem::replace(kept, Unfit::absent());
            *kept = step(unfit);
        }
    });
}

/// The skips noted since `mark`, taken out.
#[derive(Default)]
pub(super) struct Taken(Skips);

pub(super) fn take(mark: Mark) -> Taken {
    with(|s| {
        let kept = s.kept.split_off(mark.kept.min(s.kept.len()));
        let dropped = mem::replace(&mut s.dropped, mark.dropped).saturating_sub(mark.dropped);
        Taken(Skips { kept, dropped })
    })
    .unwrap_or_default()
}

/// Notes `taken` again.
pub(super) fn restore(taken: Taken) {
    with(|s| {
        taken.0.kept.into_iter().for_each(|u| s.note(u));
        s.dropped = s.dropped.saturating_add(taken.0.dropped);
    });
}

/// Puts back the slot of the decode that was reading, when this one ends or unwinds.
struct Previous(Option<Skips>);

impl Drop for Previous {
    fn drop(&mut self) {
        exchange(self.0.take());
    }
}

/// What `read` gives, with the skips noted while it ran, each led by `lead`.
pub(super) fn collect<T, E>(
    read: impl FnOnce() -> Result<T, E>,
    lead: impl Fn(Unfit) -> Unfit,
) -> Result<(T, Skips), E> {
    let _previous = Previous(exchange(Some(Skips::default())));
    let value = read()?;
    let mut skips = exchange(None).unwrap_or_default();
    skips.kept = skips.kept.into_iter().map(lead).collect();
    Ok((value, skips))
}

/// Each kept skip, its place then why, separated by `; `, then how many came after them.
impl std::fmt::Display for Skips {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, unfit) in self.kept.iter().enumerate() {
            let sep = if i == 0 { "" } else { "; " };
            write!(f, "{sep}{unfit}")?;
        }
        match self.dropped {
            0 => Ok(()),
            n => write!(f, " and {n} more"),
        }
    }
}
