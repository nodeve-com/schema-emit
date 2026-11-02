//! Where emitted code is, for the failures it reports.

use super::{recursion, STD};

/// Where the code being written is: the path so far, the list indices around it, and a counter
/// naming locals.
pub(super) struct At {
    pub path: String,
    pub indices: Vec<String>,
    pub next: usize,
    /// Set where the code is a recursive model's function (`recursion.rs`).
    pub mark: Option<recursion::Mark>,
}

impl At {
    pub(super) fn new(path: &str) -> At {
        At {
            path: path.into(),
            indices: Vec::new(),
            next: 0,
            mark: None,
        }
    }

    pub(super) fn local(&mut self, prefix: &str) -> String {
        let n = self.next;
        self.next = self.next.saturating_add(1);
        format!("{prefix}{n}")
    }

    /// The place here as a Rust expression of `Trail`, one index pushed per list around it.
    pub(super) fn trail(&self) -> String {
        let pushes: String = self
            .indices
            .iter()
            .map(|i| format!(".push({i} as u32)"))
            .collect();
        match self.mark {
            None => format!("{STD}::Trail::new(){pushes}"),
            Some(_) => format!("pre.push(n).join(reps){pushes}"),
        }
    }

    /// `r.fail(…)` for the value here.
    pub(super) fn fail(&self, reason: &str) -> String {
        if self.mark.is_some() {
            return format!(
                "r.fail({:?}, {}.as_slice(), {STD}::Reason::{reason});",
                self.path,
                self.trail()
            );
        }
        format!(
            "r.fail({:?}, &[{}], {STD}::Reason::{reason});",
            self.path,
            self.indices
                .iter()
                .map(|i| format!("{i} as u32"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    /// The place of field or key `step` of the value here, for a failure alone.
    pub(super) fn under(&self, step: &str) -> At {
        At {
            path: format!("{}.{step}", self.path),
            indices: self.indices.clone(),
            next: 0,
            mark: self.mark.clone(),
        }
    }
}
