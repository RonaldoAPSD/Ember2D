// editor/commands.rs — Undo/redo command history for the level editor.
//
// Every edit the user makes is recorded as a Command so it can be reversed
// (undo) and re-applied (redo). Two stacks are maintained:
//
//   undo stack — grows with every new user action
//   redo stack — grows when the user undoes; cleared by any new action
//
// push()     → new action: push to undo, clear redo
// pop_undo() → moves the top command from undo → redo, returns it
// pop_redo() → moves the top command from redo → undo, returns it
//
// The caller in mod.rs is responsible for applying/reversing the command's
// effect on the grid after popping.

use crate::level::TileRecord;

// ── Command ───────────────────────────────────────────────────────────────────

/// One reversible editor action.
#[derive(Debug, Clone)]
pub enum Command {
    /// Place or replace a single tile.
    ///
    /// Undo: remove `after`, restore `before`.
    /// Redo: restore `after` (ignoring `before`).
    PlaceTile {
        before: Option<TileRecord>,
        after:  TileRecord,
    },

    /// Erase a single tile.
    ///
    /// Undo: put `before` back.
    /// Redo: erase at `before.x / before.y` again.
    EraseTile {
        before: TileRecord,
    },

    /// A multi-cell batch edit recorded as one undoable step.
    ///
    /// Used by: rectangle fill, flood fill, paste, script attachment.
    ///
    /// Each entry is `(x, y, before, after)`:
    ///   - `before = None` → cell was empty before the edit
    ///   - `after  = None` → cell should be erased (used for rect-erase)
    ///
    /// Undo: for each cell, restore `before` (place or erase).
    /// Redo: for each cell, apply `after` (place or erase).
    Batch {
        cells: Vec<(i32, i32, Option<TileRecord>, Option<TileRecord>)>,
    },
}

// ── UndoStack ─────────────────────────────────────────────────────────────────

/// How many undo steps to keep in memory.
const MAX_UNDO: usize = 200;

/// Bounded undo/redo command stacks for the level editor.
pub struct UndoStack {
    /// Chronological undo history. Most recent command is at the back.
    undo: Vec<Command>,
    /// Commands that have been undone and can be re-applied. Most recent at back.
    redo: Vec<Command>,
}

impl UndoStack {
    pub fn new() -> Self {
        UndoStack { undo: Vec::new(), redo: Vec::new() }
    }

    /// Push a new user action onto the undo stack.
    ///
    /// Clears the redo stack — once you take a new action, the undone
    /// history is gone (standard linear-history model).
    pub fn push(&mut self, cmd: Command) {
        self.redo.clear();
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push(cmd);
    }

    /// Pop the most recent undo command and move it to the redo stack.
    ///
    /// Returns the command so the caller can reverse its effect on the grid.
    /// Returns None if there is nothing to undo.
    pub fn pop_undo(&mut self) -> Option<Command> {
        let cmd = self.undo.pop()?;
        self.redo.push(cmd.clone());
        Some(cmd)
    }

    /// Pop the most recent redo command and move it back to the undo stack.
    ///
    /// Returns the command so the caller can re-apply its effect on the grid.
    /// Returns None if there is nothing to redo.
    pub fn pop_redo(&mut self) -> Option<Command> {
        let cmd = self.redo.pop()?;
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push(cmd.clone());
        Some(cmd)
    }

    /// True if there is at least one action to undo.
    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }

    /// True if there is at least one action to redo.
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }

    /// Number of undo steps available.
    pub fn len(&self) -> usize { self.undo.len() }

    /// Number of redo steps available.
    pub fn redo_len(&self) -> usize { self.redo.len() }
}
