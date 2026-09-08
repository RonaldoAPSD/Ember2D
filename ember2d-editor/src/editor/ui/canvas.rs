// editor/ui/canvas.rs — Drawing functions for the editor canvas.

use super::rect::UiRect;
use crate::editor::grid::LevelGrid;
use ember2d::renderer::{color::Color, Renderer};
use ember2d_sim::level::TileRecord;

/// Width/height of one character cell in pixels, re-exported from
/// `ember2d::renderer` (Phase 7 Part 1e, docs/ember2d-phase7-plan.md, E2).
/// `grid_to_pixel` and `draw_scaled_tile` below used to hardcode `8.0`/
/// `16.0` (and `8`/`16`) independently of every other file that needed the
/// same constant — see `ui/rect.rs`'s header comment for the full history.
const CELL_W: f32 = ember2d::renderer::CELL_W as f32;
const CELL_H: f32 = ember2d::renderer::CELL_H as f32;

/// `gx`/`gy` are grid (tile) coordinates; the return value is the pixel
/// position of that tile's top-left corner. `viewport` is the Viewport
/// panel's own content rect in pixels (7C-3, master plan §5.3, E4) —
/// replaces the deleted `Layout.canvas_x`/`canvas_y`, which used to rebuild
/// an independently-computed cell-based copy of the same origin every
/// frame; adding `viewport.x`/`.y` directly (already pixels) is simpler
/// than the old `canvas_x as i32 * CELL_W as i32` reconversion, not just
/// equivalent to it.
pub fn grid_to_pixel(gx: i32, gy: i32, scroll: (f32, f32), zoom: f32, viewport: UiRect) -> (i32, i32) {
    let px = ((gx as f32 - scroll.0) * CELL_W * zoom).round() as i32;
    let py = ((gy as f32 - scroll.1) * CELL_H * zoom).round() as i32;
    (px + viewport.x.round() as i32, py + viewport.y.round() as i32)
}

#[allow(clippy::too_many_arguments)]
pub fn draw_scaled_tile(
    renderer: &mut Renderer,
    gx: i32,
    gy: i32,
    glyph: char,
    fg: Color,
    bg: Color,
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    let (px, py) = grid_to_pixel(gx, gy, scroll, zoom, viewport);

    // ── Performance Skip (Entirely off-screen check) ──────────────────────
    let cx = viewport.x.round() as i32;
    let cy = viewport.y.round() as i32;
    let cw = viewport.w.round() as i32;
    let ch = viewport.h.round() as i32;

    let tw = (CELL_W * zoom).ceil() as i32;
    let th = (CELL_H * zoom).ceil() as i32;

    // If the tile is ENTIRELY outside the viewport panel, skip it.
    // If it's partially inside, the hardware scissor will handle the clipping.
    if px + tw <= cx || px >= cx + cw || py + th <= cy || py >= cy + ch {
        return;
    }

    renderer.draw_char_scaled_pixels(px, py, glyph, fg, bg, zoom);
}

pub fn draw_grid(
    renderer: &mut Renderer,
    grid: &LevelGrid,
    active_layer: u8,
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    // Optimized range: only iterate tiles potentially on screen. `viewport`
    // is pixels; `/ CELL_W`/`/ CELL_H` first recovers the cell-space width
    // the old `Layout.canvas_w`/`canvas_h` stored directly.
    let cw_grid = (viewport.w / CELL_W / zoom).ceil() as i32;
    let ch_grid = (viewport.h / CELL_H / zoom).ceil() as i32;

    let x0 = scroll.0.floor() as i32 - 1;
    let y0 = scroll.1.floor() as i32 - 1;
    let x1 = x0 + cw_grid + 2;
    let y1 = y0 + ch_grid + 2;

    for l in 0..3 {
        for (&(gx, gy, lyr), tile) in &grid.tiles {
            if lyr != l {
                continue;
            }
            if gx < x0 || gx > x1 || gy < y0 || gy > y1 {
                continue;
            }

            let (mut fg, mut bg) = (tile.fg, tile.bg);
            if lyr != active_layer {
                fg = dim_color(fg);
                if bg != Color::Reset {
                    bg = dim_color(bg);
                }
            }
            draw_scaled_tile(renderer, gx, gy, tile.glyph, fg, bg, scroll, zoom, viewport);
        }
    }
}

fn dim_color(c: Color) -> Color {
    match c {
        Color::White => Color::Grey,
        Color::Grey => Color::DarkGrey,
        Color::Red => Color::DarkRed,
        Color::Green => Color::DarkGreen,
        Color::Blue => Color::DarkBlue,
        Color::Yellow => Color::DarkYellow,
        Color::Cyan => Color::DarkCyan,
        Color::Magenta => Color::DarkMagenta,
        _ => Color::DarkGrey,
    }
}

pub fn draw_grid_overlay(
    renderer: &mut Renderer,
    grid: &LevelGrid,
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    // Visibility range in grid cells
    let cw_grid = (viewport.w / CELL_W / zoom).ceil() as i32;
    let ch_grid = (viewport.h / CELL_H / zoom).ceil() as i32;

    let start_gx = scroll.0.floor() as i32;
    let start_gy = scroll.1.floor() as i32;

    for gy in start_gy..(start_gy + ch_grid + 1) {
        for gx in start_gx..(start_gx + cw_grid + 1) {
            let on_col = gx % 5 == 0;
            let on_row = gy % 5 == 0;
            if !on_col && !on_row {
                continue;
            }
            if (0..3).any(|l| grid.get(gx, gy, l).is_some()) {
                continue;
            }
            let ch = if on_col && on_row { '+' } else { '.' };
            draw_scaled_tile(renderer, gx, gy, ch, Color::DarkGrey, Color::Reset, scroll, zoom, viewport);
        }
    }
}

pub fn draw_void(
    renderer: &mut Renderer,
    grid: &LevelGrid,
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    let cw_grid = (viewport.w / CELL_W / zoom).ceil() as i32;
    let ch_grid = (viewport.h / CELL_H / zoom).ceil() as i32;

    let start_gx = scroll.0.floor() as i32;
    let start_gy = scroll.1.floor() as i32;

    for gy in start_gy..(start_gy + ch_grid + 1) {
        for gx in start_gx..(start_gx + cw_grid + 1) {
            if !grid.in_bounds(gx, gy) {
                draw_scaled_tile(renderer, gx, gy, ' ', Color::Reset, Color::Black, scroll, zoom, viewport);
            }
        }
    }
}

pub fn draw_level_boundary(
    renderer: &mut Renderer,
    grid: &LevelGrid,
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    let gw = grid.width as i32;
    let gh = grid.height as i32;
    for gy in 0..gh {
        draw_scaled_tile(renderer, gw, gy, '|', Color::DarkGrey, Color::Reset, scroll, zoom, viewport);
    }
    for gx in 0..gw {
        draw_scaled_tile(renderer, gx, gh, '-', Color::DarkGrey, Color::Reset, scroll, zoom, viewport);
    }
    draw_scaled_tile(renderer, gw, gh, '+', Color::DarkGrey, Color::Reset, scroll, zoom, viewport);
}

pub fn draw_cursor_highlight(
    renderer: &mut Renderer,
    mouse: &ember2d::mouse::MouseState,
    palette: &crate::editor::palette::TilePalette,
    select_mode: bool,
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    if !mouse.in_bounds || !viewport.contains(mouse.pixel_x, mouse.pixel_y) {
        return;
    }

    // Find grid cell under mouse (inverse of pixel math) — must fold in
    // `scroll` exactly like `impl_state.rs::mouse_to_grid` does, and the
    // draw call below must use the real `scroll` too (Phase 7 Part 1e,
    // docs/ember2d-phase7-plan.md, E1). Before this fix the two zeroed
    // `scroll` out entirely: at whole-number scroll that's indistinguishable
    // from correct (an integer offset cancels between the inverse and the
    // draw), but during a smooth pan `scroll` sits at a fractional value —
    // dropping it made the highlight snap to whichever whole cell the
    // fraction happened to round toward while every other draw call in this
    // file (`draw_grid`, the marker, etc.) kept drawing at the true
    // fractional offset, so the highlight visibly drifted off the tile it
    // was supposed to be sitting on for the length of the pan.
    //
    // 7C-3 (master plan §5.3, E4): reads the mouse's true sub-cell pixel
    // position (`mouse.pixel_x`/`pixel_y`) against `viewport` directly,
    // rather than `mouse.cell_x`/`cell_y` against the deleted `Layout`'s
    // cell-quantized `canvas_x`/`canvas_y` — the exact same formula
    // `mouse_to_grid` (`impl_state/mod.rs`) now uses, so the two can never
    // disagree about which tile the mouse is over, at any zoom or scroll.
    let local_x = (mouse.pixel_x - viewport.x) / CELL_W;
    let local_y = (mouse.pixel_y - viewport.y) / CELL_H;
    let gx = (local_x / zoom + scroll.0).floor() as i32;
    let gy = (local_y / zoom + scroll.1).floor() as i32;

    if select_mode {
        // Correct color for select mode as per plan: stark Dark Blue background for visibility
        draw_scaled_tile(renderer, gx, gy, '+', Color::Yellow, Color::DarkBlue, scroll, zoom, viewport);
    } else {
        let tile = palette.current();
        // Correct color for paint mode as per plan: stark White background to make it pop
        draw_scaled_tile(renderer, gx, gy, tile.glyph, tile.fg, Color::White, scroll, zoom, viewport);
    }
}

pub fn draw_spawn_marker(
    renderer: &mut Renderer,
    spawn: (f32, f32),
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    draw_scaled_tile(
        renderer,
        spawn.0.round() as i32,
        spawn.1.round() as i32,
        '@',
        Color::Green,
        Color::Reset,
        scroll,
        zoom,
        viewport,
    );
}

pub fn draw_extra_spawns(
    renderer: &mut Renderer,
    spawns: &[(String, f32, f32)],
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    // Cell-space canvas bounds, derived from `viewport` (pixels) — the
    // label below is drawn via `Renderer::draw_str`, which is still
    // cell-based, so this stays in cells rather than switching the bounds
    // check itself to `viewport.contains` (7C-3, master plan §5.3, E4).
    let canvas_x = (viewport.x / CELL_W).round() as usize;
    let canvas_y = (viewport.y / CELL_H).round() as usize;
    let canvas_w = (viewport.w / CELL_W).round() as usize;
    let canvas_h = (viewport.h / CELL_H).round() as usize;

    for (name, x, y) in spawns {
        let gx = x.round() as i32;
        let gy = y.round() as i32;
        draw_scaled_tile(renderer, gx, gy, '!', Color::Magenta, Color::Reset, scroll, zoom, viewport);

        let (px, py) = grid_to_pixel(gx, gy, scroll, zoom, viewport);
        let label: String = name.chars().take(3).collect();
        // Clipping for label. The label sits one marker-WIDTH to the right
        // of the marker glyph, not a flat +1 character cell (Phase 7 Part
        // 1e, docs/ember2d-phase7-plan.md, E3) — the marker itself is drawn
        // `CELL_W * zoom` pixels wide (`draw_scaled_tile` scales it), so at
        // zoom 1.0 that's exactly one cell and the old flat `+1` happened to
        // match, but at any other zoom the marker's on-screen footprint no
        // longer lines up with a single character cell and the label ended
        // up overlapping it instead of sitting beside it.
        let lx = (px as f32 / CELL_W + zoom).round() as usize;
        let ly = (py as f32 / CELL_H).round() as usize;
        if lx >= canvas_x && lx < canvas_x + canvas_w && ly >= canvas_y && ly < canvas_y + canvas_h {
            renderer.draw_str(lx, ly, &label, Color::Magenta, Color::Reset);
        }
    }
}

pub fn draw_rect_preview(
    renderer: &mut Renderer,
    anchor: (i32, i32),
    current: (i32, i32),
    glyph: char,
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    let x0 = anchor.0.min(current.0);
    let y0 = anchor.1.min(current.1);
    let x1 = anchor.0.max(current.0);
    let y1 = anchor.1.max(current.1);
    for gy in y0..=y1 {
        for gx in x0..=x1 {
            draw_scaled_tile(renderer, gx, gy, glyph, Color::Black, Color::White, scroll, zoom, viewport);
        }
    }
}

pub fn draw_line_preview(
    renderer: &mut Renderer,
    anchor: (i32, i32),
    current: (i32, i32),
    glyph: char,
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    for (gx, gy) in bresenham(anchor, current) {
        draw_scaled_tile(renderer, gx, gy, glyph, Color::Black, Color::Cyan, scroll, zoom, viewport);
    }
}

pub fn draw_selection_preview(
    renderer: &mut Renderer,
    anchor: (i32, i32),
    current: (i32, i32),
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    let x0 = anchor.0.min(current.0);
    let y0 = anchor.1.min(current.1);
    let x1 = anchor.0.max(current.0);
    let y1 = anchor.1.max(current.1);

    let fg = Color::Yellow;
    let bg = Color::DarkBlue;

    for gy in y0..=y1 {
        for gx in x0..=x1 {
            let is_corner = (gx == x0 || gx == x1) && (gy == y0 || gy == y1);
            let is_edge = gx == x0 || gx == x1 || gy == y0 || gy == y1;

            if is_corner {
                let ch = if gx == x0 && gy == y0 {
                    '┌'
                } else if gx == x1 && gy == y0 {
                    '┐'
                } else if gx == x0 && gy == y1 {
                    '└'
                } else {
                    '┘'
                };
                draw_scaled_tile(renderer, gx, gy, ch, fg, bg, scroll, zoom, viewport);
            } else if is_edge {
                let ch = if gx == x0 || gx == x1 { '│' } else { '─' };
                draw_scaled_tile(renderer, gx, gy, ch, fg, Color::Reset, scroll, zoom, viewport);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_paste_preview(
    renderer: &mut Renderer,
    clipboard: &[(i32, i32, TileRecord)],
    cursor: (i32, i32),
    flip_x: bool,
    flip_y: bool,
    rotate: i32,
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    let max_dx = clipboard.iter().map(|(dx, _, _)| *dx).max().unwrap_or(0);
    let max_dy = clipboard.iter().map(|(_, dy, _)| *dy).max().unwrap_or(0);
    for (dx, dy, tile) in clipboard {
        let (tdx, tdy) = transform_offset(*dx, *dy, max_dx, max_dy, flip_x, flip_y, rotate);
        draw_scaled_tile(
            renderer,
            cursor.0 + tdx,
            cursor.1 + tdy,
            tile.glyph,
            Color::Black,
            Color::Yellow,
            scroll,
            zoom,
            viewport,
        );
    }
}

pub fn bresenham(a: (i32, i32), b: (i32, i32)) -> Vec<(i32, i32)> {
    let (mut x0, mut y0) = a;
    let (x1, y1) = b;
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let mut points = Vec::new();
    loop {
        points.push((x0, y0));
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x0 += sx;
        }
        if e2 < dx {
            err += dx;
            y0 += sy;
        }
    }
    points
}

pub fn transform_offset(
    dx: i32,
    dy: i32,
    max_dx: i32,
    max_dy: i32,
    flip_x: bool,
    flip_y: bool,
    rotate: i32,
) -> (i32, i32) {
    let (dx, dy) = if flip_x { (max_dx - dx, dy) } else { (dx, dy) };
    let (dx, dy) = if flip_y { (dx, max_dy - dy) } else { (dx, dy) };
    match rotate % 4 {
        0 => (dx, dy),
        1 => (max_dy - dy, dx),
        2 => (max_dx - dx, max_dy - dy),
        3 => (dy, max_dx - dx),
        _ => (dx, dy),
    }
}

pub fn draw_physics_overlay(
    renderer: &mut Renderer,
    grid: &LevelGrid,
    active_layer: u8,
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    for l in 0..3 {
        for (&(gx, gy, lyr), tile) in &grid.tiles {
            if lyr != l {
                continue;
            }
            let is_exit = tile.next_level.is_some();
            if !tile.solid && !tile.trigger && !is_exit {
                continue;
            }
            let (mut fg, mut bg) = if is_exit {
                (Color::White, Color::Cyan)
            } else if tile.solid {
                (Color::White, Color::DarkRed)
            } else {
                (Color::Black, Color::DarkYellow)
            };
            if lyr != active_layer {
                fg = dim_color(fg);
                bg = dim_color(bg);
            }
            draw_scaled_tile(renderer, gx, gy, tile.glyph, fg, bg, scroll, zoom, viewport);
        }
    }
}

pub fn draw_erase_preview(
    renderer: &mut Renderer,
    grid_pos: (i32, i32),
    erase_size: usize,
    scroll: (f32, f32),
    zoom: f32,
    viewport: UiRect,
) {
    if erase_size <= 1 {
        return;
    }
    let half = (erase_size as i32) / 2;
    for dy in -half..=half {
        for dx in -half..=half {
            draw_scaled_tile(
                renderer,
                grid_pos.0 + dx,
                grid_pos.1 + dy,
                'X',
                Color::Red,
                Color::DarkRed,
                scroll,
                zoom,
                viewport,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact inverse of `grid_to_pixel`, working in raw sub-cell pixels
    /// rather than the whole-character-cell units `impl_state.rs::
    /// mouse_to_grid`/`draw_cursor_highlight` quantize real mouse input to
    /// first. Kept test-only and separate from those two: they only ever
    /// need to invert an already-cell-snapped screen position, but pinning
    /// `grid_to_pixel`'s own forward/inverse relationship (Phase 7 Part 1f,
    /// docs/ember2d-phase7-plan.md — the test that catches E1) means
    /// operating on the unquantized pixel value `grid_to_pixel` itself
    /// returns.
    fn pixel_to_grid(px: i32, py: i32, scroll: (f32, f32), zoom: f32, viewport: UiRect) -> (i32, i32) {
        let local_x = px as f32 - viewport.x;
        let local_y = py as f32 - viewport.y;
        let gx = (local_x / (CELL_W * zoom) + scroll.0).floor() as i32;
        let gy = (local_y / (CELL_H * zoom) + scroll.1).floor() as i32;
        (gx, gy)
    }

    #[test]
    fn grid_to_pixel_round_trips_through_its_own_inverse_across_scroll_zoom_and_viewport_origin() {
        // E1 was exactly this invariant breaking: `draw_cursor_highlight`
        // computed its grid cell with one implicit scroll (zero) and drew
        // with another (the real one), so forward and inverse silently
        // stopped agreeing at any fractional scroll. This pins that
        // `grid_to_pixel` and its inverse always agree, across every
        // combination of scroll, zoom, and viewport origin — 7C-3 (master
        // plan §5.3, E4) replaces the fixed `Layout`s this test used to
        // build with `UiRect`s standing in for the Viewport panel's own
        // rect at a couple of representative docked-panel layouts.
        let viewports = [
            UiRect::from_cells(0, 2, 80, 21),
            UiRect::from_cells(14, 3, 50, 18), // non-zero origin, e.g. a docked Hierarchy + Inspector layout
        ];
        for &viewport in &viewports {
            for &zoom in &[0.5f32, 1.0, 2.0, 3.0] {
                for &scroll in &[(0.0f32, 0.0f32), (5.0, 3.0), (2.5, 7.5), (100.0, 60.0)] {
                    for &(gx, gy) in &[(0i32, 0i32), (1, 1), (10, 4), (39, 19)] {
                        let (px, py) = grid_to_pixel(gx, gy, scroll, zoom, viewport);
                        let (gx2, gy2) = pixel_to_grid(px, py, scroll, zoom, viewport);
                        assert_eq!(
                            (gx, gy),
                            (gx2, gy2),
                            "round trip failed for zoom={}, scroll={:?}, viewport origin=({},{})",
                            zoom,
                            scroll,
                            viewport.x,
                            viewport.y
                        );
                    }
                }
            }
        }
    }
}
