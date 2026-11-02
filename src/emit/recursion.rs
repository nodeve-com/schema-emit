//! Area: recursion. A model that reaches itself through its fields can't inline, so its check is
//! a function of its own, written where the value sits: `<name>_m<index>_<n>`, emitted beside the
//! check. Its paths start at the value's path and mark the steps back to itself: `Value(.next)*.n`,
//! or with several, `Value(.left|.right)*.n`. The caller's place rides in `pre`, the repeat count
//! in `n`, and each repeat's step index (only where the mark has several steps) and list indices
//! in `reps`, so a failure inside reports its exact path through the caller's `Report` (that
//! trait's docs give the order). The stack grows with the value's depth, which its type bounds.
//!
//! A model whose cycle holds another model that reaches itself without the root (`A.b`, then
//! `B.next` back to `B`, then `B.a`) has no flat mark: the walk would nest one mark in another,
//! and `at` encodes one level. Its function is `<name>_m<index>` and reports to a `Holds`: a
//! failure inside is one `range` at the value's own path.

use super::{models, At, Emitter, STD};
use crate::ir::Model;
use crate::ir::{Base, Check, Ty};
use crate::Error;

/// Every model `ty` refers to: through its base, and through the types its checks hold.
fn of_ty(ty: &Ty, out: &mut Vec<usize>) {
    match &ty.base {
        Base::Model(m) => out.push(*m),
        Base::Union(u) => u.variants.iter().for_each(|t| of_ty(t, out)),
        Base::List(t) | Base::Map(t) => of_ty(t, out),
        Base::Tuple { prefix, rest } => {
            prefix.iter().for_each(|t| of_ty(t, out));
            rest.iter().for_each(|t| of_ty(t, out));
        }
        _ => {}
    }
    of_checks(&ty.checks, out);
}

/// Every model the types `checks` hold refer to.
fn of_checks(checks: &[Check], out: &mut Vec<usize>) {
    for check in checks {
        match check {
            Check::Contains { ty, .. } | Check::Not(ty) | Check::Field { ty, .. } => of_ty(ty, out),
            Check::ExactlyOne(subs) | Check::AtLeastOne(subs) => {
                subs.iter().for_each(|t| of_ty(t, out));
            }
            Check::Conditional {
                when,
                then,
                otherwise,
            } => [Some(when), then.as_ref(), otherwise.as_ref()]
                .into_iter()
                .flatten()
                .for_each(|t| of_ty(t, out)),
            _ => {}
        }
    }
}

impl Emitter<'_> {
    /// The models one step from `model`.
    fn next(&self, model: usize) -> Vec<usize> {
        let mut out = Vec::new();
        if let Some(m) = self.schema.models.get(model) {
            m.fields.iter().for_each(|f| of_ty(&f.ty, &mut out));
            of_checks(&m.checks, &mut out);
        }
        out
    }

    /// Every model reachable from `model` in one step or more.
    fn reach(&self, model: usize) -> Vec<usize> {
        let mut seen = Vec::new();
        let mut todo = self.next(model);
        while let Some(m) = todo.pop() {
            if !seen.contains(&m) {
                seen.push(m);
                todo.extend(self.next(m));
            }
        }
        seen
    }

    /// Whether `model` reaches itself.
    pub(super) fn cyclic(&self, model: usize) -> bool {
        self.reach(model).contains(&model)
    }

    /// The old-style function checking `model` as a value of its own.
    pub(super) fn helper(&self, model: usize) -> String {
        format!("{}_m{model}", self.tables.borrow().0)
    }
}

/// What a recursive model's function knows while it is written.
#[derive(Clone, Debug)]
pub(super) struct Mark {
    id: usize,
    name: String,
    root: usize,
    /// The steps back to the root, in the order their calls back are written.
    steps: Vec<String>,
    /// The models of the root's cycle being written in place.
    inside: Vec<usize>,
    /// Whether there is no one step to mark.
    broken: bool,
}

impl Mark {
    fn tag(&self) -> String {
        format!("\u{a7}{}\u{a7}", self.id)
    }

    /// Where a call back pushes the index of step `k`, until the number of steps is known.
    fn slot(&self, k: usize) -> String {
        format!("\u{a7}{}:{k}\u{a7}", self.id)
    }

    /// `text` with the tag as the mark, and each slot as its push where there are several steps.
    fn finish(&self, text: &str) -> String {
        let group = format!("({})*", self.steps.join("|"));
        let group = format!("{group:?}");
        let mut out = text.replace(&self.tag(), group.trim_matches('"'));
        for k in 0..self.steps.len() {
            let push = match self.steps.len() {
                1 => String::new(),
                _ => format!(".push({k} as u32)"),
            };
            out = out.replace(&self.slot(k), &push);
        }
        out
    }
}

/// A nested value of `model`, `value` a `&` of its Rust, checked by calling its function, then by
/// `checks`; `None` where `model` inlines: it is no recursion, or it is the function being written.
pub(super) fn call(
    e: &Emitter,
    at: &mut At,
    value: &str,
    model: usize,
    checks: &[Check],
) -> Option<Result<String, Error>> {
    let this = e.root.take() == Some(model);
    let m = e.schema.models.get(model)?;
    if this || !e.cyclic(model) {
        return None;
    }
    if let Some(mark) = at.mark.as_mut() {
        if mark.root == model {
            return Some(back(e, at, value, m, model, checks));
        }
        if e.reach(model).contains(&mark.root) {
            if mark.inside.contains(&model) {
                mark.broken = true;
                return Some(Ok(String::new()));
            }
            mark.inside.push(model);
            let inline = models::inline(e, at, value, model, checks);
            if let Some(mark) = at.mark.as_mut() {
                mark.inside.pop();
            }
            return Some(inline);
        }
    }
    Some(enter(e, at, value, m, model, checks))
}

/// The call of a recursive model's function from a value of `model` in another place.
fn enter(
    e: &Emitter,
    at: &mut At,
    value: &str,
    m: &Model,
    model: usize,
    checks: &[Check],
) -> Result<String, Error> {
    let whole = models::whole(e, at, value, m, model, checks)?;
    let kept = e.extra.borrow().len();
    if let Some(call) = marked(e, at, value, model)? {
        return Ok(call + &whole);
    }
    e.extra.borrow_mut().truncate(kept);
    let name = e.helper(model);
    if !e.extra.borrow().iter().any(|(n, _)| *n == name) {
        let slot = e.extra.borrow().len();
        e.extra.borrow_mut().push((name.clone(), String::new()));
        let function = e.plain_check(model, &name)?;
        if let Some(entry) = e.extra.borrow_mut().get_mut(slot) {
            entry.1 = function;
        }
    }
    let call = format!(
        "{{ let mut q = {STD}::Holds::default(); {name}({value}, &mut q); if !q.held() {{ {} }} }}\n",
        at.fail("Range")
    );
    Ok(call + &whole)
}

/// The call back to the function being written from a nested value of its model.
fn back(
    e: &Emitter,
    at: &mut At,
    value: &str,
    m: &Model,
    model: usize,
    checks: &[Check],
) -> Result<String, Error> {
    let indices: String = at
        .indices
        .iter()
        .map(|i| format!(".push({i} as u32)"))
        .collect();
    let Some(mark) = at.mark.as_mut() else {
        return Err(Error::invalid("a call back outside a recursive function"));
    };
    let step = at
        .path
        .split_once(&mark.tag())
        .map(|(_, step)| step.to_string())
        .unwrap_or_default();
    if step.contains(['|', '(', ')']) {
        mark.broken = true;
    }
    let k = match mark.steps.iter().position(|s| *s == step) {
        Some(k) => k,
        None => {
            mark.steps.push(step);
            mark.steps.len().saturating_sub(1)
        }
    };
    let slot = mark.slot(k);
    let name = mark.name.clone();
    let whole = models::whole(e, at, value, m, model, checks)?;
    Ok(format!(
        "{name}({value}, r, pre, &reps.join(&{STD}::Trail::new(){slot}{indices}), n.saturating_add(1));\n{whole}"
    ))
}

/// The call of a new function checking `value`, a `&` of recursive model `model`, in place at
/// `at`; `None` where the model has no one step to mark.
pub(super) fn marked(
    e: &Emitter,
    at: &At,
    value: &str,
    model: usize,
) -> Result<Option<String>, Error> {
    let id = e.ids.get();
    e.ids.set(id.saturating_add(1));
    let name = format!("{}_m{model}_{id}", e.tables.borrow().0);
    let mark = Mark {
        id,
        name: name.clone(),
        root: model,
        steps: Vec::new(),
        inside: Vec::new(),
        broken: false,
    };
    let tag = mark.tag();
    let mut sub = At::new(&format!("{}{tag}", at.path));
    sub.mark = Some(mark);
    let (m, rust) = e.parts(model)?;
    let root = Ty {
        checks: m.checks.clone(),
        ..Ty::of(Base::Model(model))
    };
    let kept = e.extra.borrow().len();
    let saved = e.root.replace(Some(model));
    let body = e.present(&mut sub, "v", &root);
    e.root.set(saved);
    let body = body?;
    let Some(mark) = sub.mark.filter(|m| !m.broken) else {
        return Ok(None);
    };
    let function = format!(
        "#[allow(unused_variables, unused_mut, unused_parens, clippy::all)]\n\
         pub fn {name}<R: {STD}::Report>(v: &{rust}, r: &mut R, pre: &{STD}::Trail, \
         reps: &{STD}::Trail, n: u32) {{\n{body}}}\n"
    );
    let mut extra = e.extra.borrow_mut();
    for (_, text) in extra.iter_mut().skip(kept) {
        *text = mark.finish(text);
    }
    extra.push((name.clone(), mark.finish(&function)));
    Ok(Some(format!(
        "{name}({value}, r, &{}, &{STD}::Trail::new(), 0);\n",
        at.trail()
    )))
}
