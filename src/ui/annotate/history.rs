//! Undo and redo history of what was done to the annotations.

use std::collections::VecDeque;

use super::*;

/// Steps that can be undone, the oldest go
const STEPS: usize = 200;

/// Snapshots of the annotations after each completed change. A change completes
/// when the mouse is released, nothing is being dragged, and no text is being typed,
/// so a drag, slider move or typed text counts as one step.
#[derive(Debug, Clone, Default)]
pub(super) struct History {
    /// As they were after the last change, none until they are first seen
    settled: Option<Vec<Annotation>>,
    /// As they were before each change, the last one before the last change
    undo: VecDeque<Vec<Annotation>>,
    /// As they were before each undo, the last one before the last undo
    redo: Vec<Vec<Annotation>>,
}

impl History {
    /// Keeps a step to go back to once a change is done, not while `busy`
    pub(super) fn record(&mut self, annotations: &[Annotation], busy: bool) {
        let Some(settled) = &self.settled else {
            self.settled = Some(annotations.to_vec());
            return;
        };
        if busy || settled.as_slice() == annotations {
            return;
        }
        if let Some(before) = self.settled.replace(annotations.to_vec()) {
            self.undo.push_back(before);
        }
        if self.undo.len() > STEPS {
            self.undo.pop_front();
        }
        self.redo.clear();
    }

    /// One step back, or forward again after it went back. Returns whether there was one.
    pub(super) fn step(&mut self, annotations: &mut Vec<Annotation>, back: bool) -> bool {
        let then = if back {
            self.undo.pop_back()
        } else {
            self.redo.pop()
        };
        let Some(then) = then else {
            return false;
        };
        let now = std::mem::replace(annotations, then.clone());
        if back {
            self.redo.push(now);
        } else {
            self.undo.push_back(now);
        }
        self.settled = Some(then);
        true
    }

    /// A new image: nothing to undo. Annotations switched off and on keep what can be undone.
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}
