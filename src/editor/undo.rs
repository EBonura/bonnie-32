//! Generic undo/redo stack using the memento (snapshot) pattern.
//! After each edit, push a clone. Ctrl+Z pops back; Ctrl+Shift+Z / Ctrl+Y redoes.

const MAX_HISTORY: usize = 64;

pub struct UndoStack<T: Clone> {
    /// States before the current one (oldest → newest)
    history: Vec<T>,
    /// States after the current one (available to redo)
    future: Vec<T>,
}

impl<T: Clone> UndoStack<T> {
    pub fn new() -> Self {
        Self { history: Vec::new(), future: Vec::new() }
    }

    /// Push a snapshot of the current state before an edit is applied.
    /// Clears the redo stack (new edit branches away from any future).
    pub fn push(&mut self, snapshot: T) {
        if self.history.len() >= MAX_HISTORY {
            self.history.remove(0);
        }
        self.history.push(snapshot);
        self.future.clear();
    }

    /// Undo: restores the previous state, returns it and pushes `current`
    /// onto the redo stack. Returns None if no history.
    pub fn undo(&mut self, current: T) -> Option<T> {
        let prev = self.history.pop()?;
        if self.future.len() >= MAX_HISTORY {
            self.future.remove(0);
        }
        self.future.push(current);
        Some(prev)
    }

    /// Redo: restores the next state, returns it and pushes `current`
    /// onto the history. Returns None if no future.
    pub fn redo(&mut self, current: T) -> Option<T> {
        let next = self.future.pop()?;
        if self.history.len() >= MAX_HISTORY {
            self.history.remove(0);
        }
        self.history.push(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool { !self.history.is_empty() }
    pub fn can_redo(&self) -> bool { !self.future.is_empty() }

    pub fn clear(&mut self) {
        self.history.clear();
        self.future.clear();
    }
}

impl<T: Clone> Default for UndoStack<T> {
    fn default() -> Self { Self::new() }
}
