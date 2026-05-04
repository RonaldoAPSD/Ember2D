// renderer/mod.rs — The Fake Terminal Renderer (now backed by a real OS window).
//
// ┌─────────────────────────────────────────────────────────────────────────┐
// │  WHAT CHANGED FROM THE CROSSTERM VERSION                                │
// │                                                                         │
// │  Before: we sent ANSI escape codes to a real terminal emulator.         │
// │  After:  we open our OWN window and draw pixels into it directly.       │
// │                                                                         │
// │  This means:                                                            │
// │   - No terminal needed. Works in any environment.                       │
// │   - We control every pixel, not the terminal's font renderer.           │
// │   - We embed our own font and draw each character glyph ourselves.      │
// └─────────────────────────────────────────────────────────────────────────┘
//
// ┌─────────────────────────────────────────────────────────────────────────┐
// │  HOW THE PIXEL RENDERING PIPELINE WORKS                                 │
// │                                                                         │
// │  The game world is a grid of CELLS (like a spreadsheet).                │
// │  Each Cell holds: a char ('A', '#', '@'), a fg color, and a bg color.  │
// │                                                                         │
// │  To put a Cell on screen:                                               │
// │    1. Look up the char in the embedded bitmap font → get [u8; 8]        │
// │    2. For each of the 8 rows, read 8 bits (= 8 pixels)                  │
// │    3. A 1-bit → write fg color at that pixel                             │
// │    4. A 0-bit → write bg color at that pixel                             │
// │    5. Repeat each row twice vertically (8×8 font → 8×16 cell)           │
// │                                                                         │
// │  All cells together fill a flat Vec<u32> (the PIXEL BUFFER).            │
// │  minifb blits this buffer to the screen each frame.                     │
// └─────────────────────────────────────────────────────────────────────────┘
//
// ┌─────────────────────────────────────────────────────────────────────────┐
// │  DOUBLE-BUFFER DIFF IS STILL USED                                       │
// │                                                                         │
// │  We still maintain front and back Cell buffers.                         │
// │  present() only RE-RASTERIZES cells that changed since the last frame.  │
// │  Then the full pixel buffer is sent to minifb (it can't be partial).    │
// │                                                                         │
// │  CPU work    = O(changed cells)    — typically very small               │
// │  GPU/OS work = O(total pixels)     — unavoidable full-buffer blit       │
// └─────────────────────────────────────────────────────────────────────────┘

pub mod buffer;
pub mod color;

use std::io;
use minifb::{Window, WindowOptions, Scale};
use font8x8::legacy::BASIC_LEGACY;

use buffer::{Buffer, Cell};
pub use color::{Color, DEFAULT_FG, DEFAULT_BG};

// ── Cell size constants ───────────────────────────────────────────────────────

/// Width of one character cell in pixels.
/// The embedded font glyphs are 8 pixels wide.
const CELL_W: usize = 8;

/// Height of one character cell in pixels.
/// The font glyphs are 8 pixels tall; we DOUBLE each row vertically
/// to make characters taller and easier to read (8 → 16 pixels per cell).
const CELL_H: usize = 16;

/// Window scale factor used by minifb (Scale::X2).
/// window.get_size() returns screen-space pixels, which are SCALE× the buffer pixels.
pub const SCALE: usize = 2;

// ─────────────────────────────────────────────────────────────────────────────

/// The renderer: owns the OS window, the pixel buffer, and the two cell buffers.
pub struct Renderer {
    /// The minifb window — our actual OS window.
    /// Calling `window.update_with_buffer(...)` blits our pixels to screen
    /// AND processes OS events (keyboard, window close, etc.).
    window: Window,

    /// Flat pixel buffer: `pixel_width * pixel_height` u32 values.
    ///
    /// Format per pixel: 0x00RRGGBB (bits 23-16 = R, 15-8 = G, 7-0 = B).
    /// This is the "canvas" we paint characters onto before handing it to minifb.
    pixel_buffer: Vec<u32>,

    /// The "back" Cell buffer — we draw into this each frame.
    /// Nothing is visible until present() is called.
    back: Buffer,

    /// The "front" Cell buffer — what the screen currently shows.
    /// Used for diffing: we only rasterize cells that changed.
    front: Buffer,

    /// Width of the viewport in character columns (e.g. 80).
    pub width: usize,

    /// Height of the viewport in character rows (e.g. 24).
    pub height: usize,

    /// Width of the pixel buffer: `width * CELL_W` (e.g. 640).
    pixel_width: usize,

    /// Height of the pixel buffer: `height * CELL_H` (e.g. 384).
    pixel_height: usize,
}

impl Renderer {
    /// Open the OS window and initialize the renderer.
    ///
    /// `width` and `height` are in CHARACTER CELLS (e.g. 80 × 24).
    /// The actual window size in pixels is `width*CELL_W × height*CELL_H`,
    /// then doubled by minifb's Scale::X2 for comfortable viewing on modern monitors.
    ///
    /// Returns an io::Error if the window cannot be created (e.g. no display server).
    pub fn new(width: usize, height: usize, title: &str) -> io::Result<Self> {
        let pixel_width  = width  * CELL_W;
        let pixel_height = height * CELL_H;

        // minifb::Window::new creates the actual OS window.
        //
        // WindowOptions controls how the window behaves:
        //   scale: Scale::X2 → each pixel in our buffer appears as a 2×2 block
        //          on screen. Makes the window 2× larger without us having to
        //          change our pixel buffer or rendering code at all.
        //          Our 640×384 buffer displays as a ~1280×768 window — comfortable.
        //   resize: false → fixed size window. Resizing would require us to rebuild
        //           both the pixel buffer and the Cell buffers, which we skip for now.
        let window = Window::new(
            title,
            pixel_width,
            pixel_height,
            WindowOptions {
                scale:  Scale::X2,
                resize: true,
                ..Default::default()
            },
        )
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        Ok(Renderer {
            window,
            // Fill the pixel buffer with the default background color.
            pixel_buffer: vec![DEFAULT_BG; pixel_width * pixel_height],
            back:  Buffer::new(width, height),
            front: Buffer::new(width, height),
            width,
            height,
            pixel_width,
            pixel_height,
        })
    }

    // ── Window state ──────────────────────────────────────────────────────────

    /// True while the window hasn't been closed (user hasn't clicked the X button).
    pub fn is_open(&self) -> bool {
        self.window.is_open()
    }

    /// Returns all keyboard keys that are currently held down.
    ///
    /// This reflects the state captured during the LAST call to `update_with_buffer`.
    /// The engine reads this once per frame and feeds it to the InputManager.
    pub fn current_keys(&self) -> Vec<minifb::Key> {
        self.window.get_keys()
    }

    /// Returns the current mouse position in WINDOW pixel space, or None if the
    /// cursor is outside the window.
    ///
    /// minifb accounts for Scale::X2 internally — the returned (x, y) are already
    /// in buffer pixel coordinates (range 0..pixel_width, 0..pixel_height), not in
    /// the larger on-screen window space. No scale division needed by the caller.
    /// `MouseState` in `src/mouse.rs` divides by CELL_W/CELL_H to get cell coords.
    pub fn current_mouse_pos(&self) -> Option<(f32, f32)> {
        // MouseMode::Clamp clamps the position to the window bounds (never returns
        // a position outside the window even if the cursor is at the very edge).
        self.window.get_mouse_pos(minifb::MouseMode::Clamp)
    }

    /// Returns true if the given mouse button is currently held down.
    ///
    /// minifb::MouseButton has three variants: Left, Right, Middle.
    /// Pass whichever you want to query.
    pub fn is_mouse_button_down(&self, button: minifb::MouseButton) -> bool {
        self.window.get_mouse_down(button)
    }

    /// Returns the scroll wheel delta since the last frame as (horizontal, vertical).
    /// Positive vertical = scroll down/away; negative = scroll up/toward user.
    /// Returns (0.0, 0.0) when there is no scroll input this frame.
    pub fn get_scroll_wheel(&self) -> (f32, f32) {
        self.window.get_scroll_wheel().unwrap_or((0.0, 0.0))
    }

    // ── Drawing API (writes to the BACK buffer) ───────────────────────────────

    /// Erase the back buffer. Call this at the start of every frame.
    pub fn clear(&mut self) {
        self.back.clear();
    }

    /// Place a single character at cell column `x`, row `y`.
    pub fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color) {
        self.back.set(x, y, Cell::new(ch, fg, bg));
    }

    /// Draw a string starting at (x, y), extending rightward.
    /// Characters beyond the right edge are silently clipped.
    pub fn draw_str(&mut self, x: usize, y: usize, s: &str, fg: Color, bg: Color) {
        self.back.set_str(x, y, s, fg, bg);
    }

    /// Draw multiple text lines stacked vertically, starting at (x, y).
    ///
    /// Great for multi-line ASCII art:
    /// ```
    /// renderer.draw_lines(5, 3, &["/\\", "\\/"], Color::Yellow, Color::Reset);
    /// ```
    pub fn draw_lines(&mut self, x: usize, y: usize, lines: &[&str], fg: Color, bg: Color) {
        for (row_offset, line) in lines.iter().enumerate() {
            self.back.set_str(x, y.saturating_add(row_offset), line, fg, bg);
        }
    }

    /// Draw a rectangular outline with `+` corners, `-` top/bottom, `|` sides.
    pub fn draw_rect_outline(&mut self, x: usize, y: usize, w: usize, h: usize, fg: Color, bg: Color) {
        if w < 2 || h < 2 { return; }
        for col in (x + 1)..(x + w - 1) {
            self.draw_char(col, y,       '-', fg, bg);
            self.draw_char(col, y + h - 1, '-', fg, bg);
        }
        for row in (y + 1)..(y + h - 1) {
            self.draw_char(x,       row, '|', fg, bg);
            self.draw_char(x + w - 1, row, '|', fg, bg);
        }
        self.draw_char(x,       y,       '+', fg, bg);
        self.draw_char(x + w - 1, y,       '+', fg, bg);
        self.draw_char(x,       y + h - 1, '+', fg, bg);
        self.draw_char(x + w - 1, y + h - 1, '+', fg, bg);
    }

    /// Fill a rectangle with a repeating character.
    pub fn draw_rect_filled(&mut self, x: usize, y: usize, w: usize, h: usize, ch: char, fg: Color, bg: Color) {
        for row in y..(y + h) {
            for col in x..(x + w) {
                self.draw_char(col, row, ch, fg, bg);
            }
        }
    }

    // ── Pixel rendering ───────────────────────────────────────────────────────

    /// Rasterize one cell into the pixel buffer at the correct pixel location.
    ///
    /// HOW IT WORKS, step by step:
    ///   1. Resolve `fg` and `bg` Colors to 0x00RRGGBB u32 values.
    ///   2. Get the 8×8 font bitmap for the character from BASIC_LEGACY.
    ///      (Characters outside ASCII 0-127 fall back to '?'.)
    ///   3. For each of the 8 font rows:
    ///        a. Read the row byte (8 bits = 8 pixels).
    ///        b. For each bit position (column 0-7):
    ///             - bit is 1 → write fg color to pixel buffer
    ///             - bit is 0 → write bg color to pixel buffer
    ///        c. Write the same row TWICE to produce 16 pixel rows per cell.
    ///           (Stretches 8-tall font glyphs to fill our 16-tall cells.)
    fn rasterize_cell(&mut self, cell_x: usize, cell_y: usize, cell: Cell) {
        // Resolve Color::Reset to the actual default colors.
        let fg_pixel = cell.fg.to_rgb(DEFAULT_FG);
        let bg_pixel = cell.bg.to_rgb(DEFAULT_BG);

        // Look up the font bitmap for this character.
        // BASIC_LEGACY covers ASCII 0-127; use '?' for anything outside that range.
        let char_idx = cell.ch as usize;
        let bitmap: [u8; 8] = if char_idx < 128 {
            BASIC_LEGACY[char_idx]
        } else {
            BASIC_LEGACY['?' as usize]
        };

        // Top-left pixel of this cell in the pixel buffer.
        let base_px = cell_x * CELL_W;
        let base_py = cell_y * CELL_H;

        for font_row in 0..8usize {
            let row_byte = bitmap[font_row];

            for font_col in 0..8usize {
                // Bit 0 of the byte = leftmost pixel (LSB = left in font8x8).
                // We check each bit position from 0 (left) to 7 (right).
                let pixel_on = (row_byte >> font_col) & 1 != 0;
                let pixel_color = if pixel_on { fg_pixel } else { bg_pixel };

                let px = base_px + font_col;

                // Each font row is written to TWO consecutive pixel rows to
                // vertically stretch the 8-tall glyph into a 16-tall cell.
                let py1 = base_py + font_row * 2;
                let py2 = py1 + 1;

                // Bounds check: clip rather than panic.
                if px < self.pixel_width {
                    if py1 < self.pixel_height {
                        self.pixel_buffer[py1 * self.pixel_width + px] = pixel_color;
                    }
                    if py2 < self.pixel_height {
                        self.pixel_buffer[py2 * self.pixel_width + px] = pixel_color;
                    }
                }
            }
        }
    }

    // ── Present ───────────────────────────────────────────────────────────────

    /// Flush the back buffer to the screen.
    ///
    /// Steps:
    ///   1. Diff back vs front → only rasterize cells that actually changed.
    ///   2. Write changed cells into the pixel buffer.
    ///   3. Call `window.update_with_buffer()`:
    ///        - Blits pixel buffer to the OS window
    ///        - Processes any pending OS events (keyboard, close button, etc.)
    ///   4. Swap back and front cell buffers.
    pub fn present(&mut self) -> io::Result<()> {
        // Find which cells changed since the last frame.
        // The diff returns OWNED Cell values so there's no lifetime conflict with the swap.
        let diffs = self.back.diff(&self.front);

        // Rasterize only the changed cells into the pixel buffer.
        // The pixel buffer accumulates: unchanged cells keep their pixels from before.
        for (cx, cy, cell) in diffs {
            self.rasterize_cell(cx, cy, cell);
        }

        // Blit the entire pixel buffer to the window.
        // This also pumps the OS event queue — key state is refreshed here.
        self.window
            .update_with_buffer(&self.pixel_buffer, self.pixel_width, self.pixel_height)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        // The back buffer is now the new front (what the screen currently shows).
        std::mem::swap(&mut self.front, &mut self.back);

        Ok(())
    }

    /// Check whether the OS window was resized since the last call, and if so
    /// rebuild the pixel and cell buffers to match the new dimensions.
    ///
    /// `window.get_size()` returns the window size in screen-space pixels.
    /// With Scale::X2 each buffer pixel maps to a 2×2 block on screen, so
    /// buffer_width = screen_width / SCALE, and cell_width = buffer_width / CELL_W.
    ///
    /// Returns `true` if the buffers were rebuilt (i.e. the size changed).
    pub fn try_handle_resize(&mut self) -> bool {
        let (screen_w, screen_h) = self.window.get_size();
        let new_w = (screen_w / SCALE / CELL_W).max(20);
        let new_h = (screen_h / SCALE / CELL_H).max(6);
        if new_w == self.width && new_h == self.height {
            return false;
        }
        self.width        = new_w;
        self.height       = new_h;
        self.pixel_width  = new_w * CELL_W;
        self.pixel_height = new_h * CELL_H;
        self.pixel_buffer = vec![DEFAULT_BG; self.pixel_width * self.pixel_height];
        self.back  = Buffer::new(new_w, new_h);
        self.front = Buffer::new(new_w, new_h);
        true
    }
}
