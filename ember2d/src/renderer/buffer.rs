// renderer/buffer.rs — The screen buffer.
//
// A BUFFER in rendering is a 2-D grid of data representing what the screen
// should show. We maintain TWO buffers at all times:
//
//   ┌─────────────────┐      ┌─────────────────┐
//   │  FRONT BUFFER   │      │   BACK BUFFER   │
//   │  (on screen     │      │  (we draw into  │
//   │   right now)    │      │   this frame)   │
//   └─────────────────┘      └─────────────────┘
//
// DOUBLE BUFFERING technique:
//   1. Every frame we clear the BACK buffer and draw the new frame into it.
//   2. After drawing, we DIFF the back against the front: find which cells changed.
//   3. Only the changed cells get sent to the real terminal (expensive I/O).
//   4. Back and front swap — the back becomes the new front.
//
// WHY DIFF INSTEAD OF REDRAWING EVERYTHING?
//   Sending data to a terminal is slow (it goes through the OS, shell, and
//   terminal emulator). A 80×24 screen has 1,920 cells. If only the player
//   moved, only 2 cells changed. Sending 2 cells is 960× faster than sending
//   all 1,920. The diff step pays for itself immediately.

use crate::renderer::color::Color;

// ─────────────────────────── Cell ────────────────────────────────────────────

/// One character cell in the fake terminal.
///
/// The terminal is a grid of cells. Each cell has exactly:
///   - One character (the glyph to display)
///   - A foreground color (color of the glyph)
///   - A background color (color behind the glyph)
///
/// `Cell` is `Copy` because it's small (a char + 2 enum variants) and
/// we want to copy it freely without worrying about ownership.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub scale: f32,
}

impl Cell {
    /// An empty cell: a space character with the terminal's default colors.
    /// This is what "nothing drawn here" looks like.
    pub fn empty() -> Self {
        Cell {
            ch: ' ',
            fg: Color::Reset,
            bg: Color::Reset,
            scale: 1.0,
        }
    }

    /// Create a cell with specific character and colors.
    pub fn new(ch: char, fg: Color, bg: Color) -> Self {
        Cell { ch, fg, bg, scale: 1.0 }
    }

    pub fn new_scaled(ch: char, fg: Color, bg: Color, scale: f32) -> Self {
        Cell { ch, fg, bg, scale }
    }
}

// ─────────────────────────── Buffer ──────────────────────────────────────────

/// A 2-D grid of Cells stored as a flat 1-D Vec.
///
/// MEMORY LAYOUT (row-major order):
///   The 2-D grid is flattened into a 1-D array. Row 0 occupies indices [0..width),
///   row 1 occupies indices [width..2*width), and so on.
///
///   ┌───┬───┬───┐     Index formula:
///   │ 0 │ 1 │ 2 │     index(x, y) = y * width + x
///   ├───┼───┼───┤
///   │ 3 │ 4 │ 5 │     So cell at column 2, row 1 (width=3):
///   └───┴───┴───┘     index(2, 1) = 1 * 3 + 2 = 5   ✓
pub struct Buffer {
    pub width: usize,
    pub height: usize,
    /// The flat array of cells. Length == width * height.
    cells: Vec<Cell>,
}

impl Buffer {
    /// Allocate a new buffer filled entirely with empty (space) cells.
    pub fn new(width: usize, height: usize) -> Self {
        Buffer {
            width,
            height,
            cells: vec![Cell::empty(); width * height],
        }
    }

    /// Convert 2-D (column, row) coordinates into a 1-D array index.
    ///
    /// Returns `None` if the coordinates are out of bounds so callers
    /// can choose to silently ignore out-of-bounds draws (safe, no panic).
    fn index(&self, x: usize, y: usize) -> Option<usize> {
        if x < self.width && y < self.height {
            Some(y * self.width + x)
        } else {
            None
        }
    }

    /// Get the cell at column x, row y.
    /// Returns None if the position is outside the buffer.
    pub fn get(&self, x: usize, y: usize) -> Option<Cell> {
        self.index(x, y).map(|i| self.cells[i])
    }

    /// Write a cell at column x, row y.
    /// Silently does nothing if the position is out of bounds.
    pub fn set(&mut self, x: usize, y: usize, cell: Cell) {
        if let Some(i) = self.index(x, y) {
            self.cells[i] = cell;
        }
    }

    /// Write a string of characters starting at (x, y) going rightward.
    ///
    /// Characters that would fall outside the right edge are clipped (ignored).
    /// This is how we draw text labels without worrying about buffer overruns.
    pub fn set_str(&mut self, x: usize, y: usize, s: &str, fg: Color, bg: Color) {
        for (i, ch) in s.chars().enumerate() {
            // x + i could wrap around or exceed width, but `set` handles that.
            self.set(x.saturating_add(i), y, Cell::new(ch, fg, bg));
        }
    }

    /// Reset every cell to empty.
    /// Called at the start of every frame before drawing the new scene.
    pub fn clear(&mut self) {
        for cell in self.cells.iter_mut() {
            *cell = Cell::empty();
        }
    }

    /// Compare this buffer against `other` and return every cell position
    /// where they differ, along with the new cell value (from `self`).
    ///
    /// The result drives the renderer's terminal output:
    /// only diffed cells need to be written, keeping I/O minimal.
    ///
    /// We return owned `Cell` values (not references) because the caller
    /// needs to use the data after the buffers potentially swap.
    pub fn diff(&self, other: &Buffer) -> Vec<(usize, usize, Cell)> {
        let mut diffs = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = y * self.width + x;
                if self.cells[idx] != other.cells[idx] {
                    diffs.push((x, y, self.cells[idx]));
                }
            }
        }
        diffs
    }
}
