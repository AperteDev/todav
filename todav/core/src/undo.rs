//! What local edits replaced, so they can be undone: per step (one user action), the calendar
//! data each item it touched had before.

use crate::store::Kind;

/// Steps kept; older ones are forgotten.
const KEEP: usize = 100;

/// An item as it was before a step: its data, or None when it did not exist.
pub(crate) struct Before {
    pub kind: Kind,
    pub list_href: String,
    pub uid: String,
    pub raw_ics: Option<String>,
}

struct Step {
    id: u64,
    before: Vec<Before>,
}

#[derive(Default)]
pub(crate) struct Journal {
    steps: Vec<Step>,
    /// Nesting of `begin`; above 0, changes go into the last step.
    depth: usize,
    next: u64,
}

impl Journal {
    /// From now until the matching `end`, changes make one step.
    pub fn begin(&mut self) {
        if self.depth == 0 {
            self.open();
        }
        self.depth += 1;
    }

    pub fn end(&mut self) {
        self.depth -= 1;
        if self.depth == 0 && self.steps.last().is_some_and(|s| s.before.is_empty()) {
            self.steps.pop();
        }
    }

    fn open(&mut self) {
        self.next += 1;
        self.steps.push(Step {
            id: self.next,
            before: Vec::new(),
        });
        if self.steps.len() > KEEP {
            self.steps.remove(0);
        }
    }

    /// An item before a change: into the open step, else a step of its own. Within a step only
    /// the first state of an item counts.
    pub fn record(&mut self, b: Before) {
        if self.depth == 0 {
            self.open();
        }
        let step = self.steps.last_mut().expect("a step is open");
        let same = |x: &Before| x.kind == b.kind && x.list_href == b.list_href && x.uid == b.uid;
        if !step.before.iter().any(same) {
            step.before.push(b);
        }
    }

    /// Drop what was in list `href`, now deleted: there is nowhere to put it back.
    pub fn forget_list(&mut self, href: &str) {
        for s in &mut self.steps {
            s.before.retain(|b| b.list_href != href);
        }
        self.steps.retain(|s| !s.before.is_empty());
    }

    pub fn last(&self) -> Option<u64> {
        self.steps.last().map(|s| s.id)
    }

    /// Remove step `id` (else the latest) and return what it replaced, oldest change first.
    pub fn take(&mut self, id: Option<u64>) -> Option<Vec<Before>> {
        let i = match id {
            Some(id) => self.steps.iter().position(|s| s.id == id)?,
            None => self.steps.len().checked_sub(1)?,
        };
        Some(self.steps.remove(i).before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn before(uid: &str, ics: &str) -> Before {
        Before {
            kind: Kind::Task,
            list_href: "l".into(),
            uid: uid.into(),
            raw_ics: Some(ics.into()),
        }
    }

    #[test]
    fn grouped_changes_undo_together_and_keep_the_first_state() {
        let mut j = Journal::default();
        j.record(before("a", "a0"));
        let alone = j.last();
        j.begin();
        j.record(before("b", "b0"));
        j.begin();
        j.record(before("b", "b1"));
        j.record(before("c", "c0"));
        j.end();
        j.end();
        // An empty group leaves no step.
        j.begin();
        j.end();
        let step = j.take(None).unwrap();
        let got: Vec<_> = step.iter().map(|b| b.raw_ics.clone().unwrap()).collect();
        assert_eq!(got, ["b0", "c0"]);
        assert_eq!(j.take(alone).unwrap()[0].uid, "a");
        assert!(j.take(None).is_none());
        assert!(j.take(alone).is_none(), "a step is undone once");
    }
}
