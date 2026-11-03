//! The Rust path of each model's struct.

use super::StdForm;

impl StdForm<'_> {
    /// Model `m`'s struct name as the emitted code spells it: the name function's, with `_` added while an earlier struct of
    /// this crate holds it. A model written elsewhere keeps its name.
    pub fn model_name(&self, m: usize) -> String {
        let mut taken: Vec<String> = Vec::new();
        for j in 0..m {
            if (self.external)(j).is_none() {
                let id = self.unused(&taken, (self.name)(j));
                taken.push(id);
            }
        }
        match (self.external)(m) {
            Some(_) => (self.name)(m),
            None => self.unused(&taken, (self.name)(m)),
        }
    }

    /// `id` with `_` added while `taken` holds it.
    fn unused(&self, taken: &[String], mut id: String) -> String {
        while taken.contains(&id) {
            id.push('_');
        }
        id
    }

    /// Model `m`'s struct, through its module where another crate writes it.
    pub(super) fn model_path(&self, m: usize) -> String {
        match (self.external)(m) {
            Some(module) => format!("{module}::{}", self.model_name(m)),
            None => self.model_name(m),
        }
    }
}
