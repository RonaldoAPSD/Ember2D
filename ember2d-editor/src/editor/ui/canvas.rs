// editor/ui/canvas.rs — Drawing functions for the editor canvas.

use super::rect::UiRect;
use crate::editor::grid::LevelGrid;
use crate::editor::sprites::SpriteAssets;
use ember2d::renderer::{color::Color, DrawSurface};
use ember2d_sim::level::TileRecord;

/// Width/height of one character cell in pixels, re-exported from
/// `ember2d::renderer` (Phase 7 Part 1e, docs/ember2d-phase7-plan.md, E2).
/// `grid_to_pixel` and `draw_scaled_tile` below used to hardcode `8.0`/
/// `16.0` (and `8`/`16`) independently of every other file that needed the
/// same constant — see `ui/rect.rs`'s header comment for the full history.
const CELL_W: f32 = ember2d::renderer::CELL_W as f32;
const CELL_H: f32 = ember2d::renderer::CELL_H as f32;

/// Where the canvas is looking and how big a grid cell is on screen —
/// everything a canvas draw or hit-test needs to map a grid cell to a pixel
/// and back. Step 9-5 (docs/ember2d-master-plan.md §5.8) bundled the
/// `(scroll, zoom, viewport)` triple every function here used to take and
/// added `cell`: one level cell in logical pixels at zoom 1, which is the
/// project's world cell (`ProjectData::world_cell`) — 8×16 (the glyph cell)
/// unless a sprite project sets it square, so square sprites draw square
/// in the editor exactly as they do in play mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasView {
    pub scroll: (f32, f32),
    pub zoom: f32,
    /// The Viewport panel's own content rect in logical pixels (7C-3,
    /// master plan §5.3, E4) — replaces the deleted `Layout.canvas_x`/
    /// `canvas_y`, which used to rebuild an independently-computed
    /// cell-based copy of the same origin every frame.
    pub viewport: UiRect,
    pub cell: (f32, f32),
}

impl CanvasView {
    /// A view with the classic 8×16 cell (tests, and any project that
    /// doesn't set `world_cell`).
    pub fn new(scroll: (f32, f32), zoom: f32, viewport: UiRect) -> Self {
        CanvasView { scroll, zoom, viewport, cell: (CELL_W, CELL_H) }
    }

    /// One grid cell's on-screen size in logical pixels.
    pub fn cell_px(&self) -> (f32, f32) {
        (self.cell.0 * self.zoom, self.cell.1 * self.zoom)
    }

    /// The per-axis glyph scale that fills one grid cell — `(zoom, zoom)`
    /// for an 8×16 cell, `(2·zoom, zoom)` for 16×16.
    pub fn glyph_size(&self) -> [f32; 2] {
        let (w, h) = self.cell_px();
        [w / CELL_W, h / CELL_H]
    }

    /// `gx`/`gy` are grid (tile) coordinates; the return value is the pixel
    /// position of that tile's top-left corner.
    pub fn grid_to_pixel(&self, gx: i32, gy: i32) -> (i32, i32) {
        let (cw, ch) = self.cell_px();
        let px = ((gx as f32 - self.scroll.0) * cw).round() as i32;
        let py = ((gy as f32 - self.scroll.1) * ch).round() as i32;
        (px + self.viewport.x.round() as i32, py + self.viewport.y.round() as i32)
    }

    /// The grid cell under a logical pixel position — `grid_to_pixel`'s
    /// inverse, and the one formula the cursor highlight, `mouse_to_grid`
    /// and the zoom pivot all share (E1: they must never disagree).
    pub fn pixel_to_grid(&self, px: f32, py: f32) -> (i32, i32) {
        let (cw, ch) = self.cell_px();
        let gx = ((px - self.viewport.x) / cw + self.scroll.0).floor() as i32;
        let gy = ((py - self.viewport.y) / ch + self.scroll.1).floor() as i32;
        (gx, gy)
    }

    /// How many grid cells the viewport spans, rounded up.
    pub fn visible_cells(&self) -> (i32, i32) {
        let (cw, ch) = self.cell_px();
        ((self.viewport.w / cw).ceil() as i32, (self.viewport.h / ch).ceil() as i32)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_scaled_tile(
    renderer: &mut dyn DrawSurface,
    gx: i32,
    gy: i32,
    glyph: char,
    fg: Color,
    bg: Color,
    view: &CanvasView,
) {
    let (px, py) = view.grid_to_pixel(gx, gy);

    // ── Performance Skip (Entirely off-screen check) ──────────────────────
    let cx = view.viewport.x.round() as i32;
    let cy = view.viewport.y.round() as i32;
    let cw = view.viewport.w.round() as i32;
    let ch = view.viewport.h.round() as i32;

    let tw = view.cell_px().0.ceil() as i32;
    let th = view.cell_px().1.ceil() as i32;

    // If the tile is ENTIRELY outside the viewport panel, skip it.
    // If it's partially inside, the hardware scissor will handle the clipping.
    if px + tw <= cx || px >= cx + cw || py + th <= cy || py >= cy + ch {
        return;
    }

    renderer.draw_char_sized_pixels(px, py, glyph, fg, bg, view.glyph_size());
}

/// Step 8-2 (docs/ember2d-master-plan.md §5.7): a tile with a `sprite`
/// that `sprites` can resolve is drawn as that tileset region, stretched
/// over its cell exactly the way play mode draws it (one cell, not the
/// sheet's natural size); a tile whose tileset or region is missing keeps
/// drawing its glyph, so a broken reference is visible rather than blank.
#[allow(clippy::too_many_arguments)]
pub fn draw_grid(
    renderer: &mut dyn DrawSurface,
    grid: &LevelGrid,
    sprites: &SpriteAssets,
    anim_time: f32,
    active_layer: u8,
    view: &CanvasView,
) {
    // Optimized range: only iterate tiles potentially on screen — the
    // viewport's size in grid cells, which the old `Layout.canvas_w`/
    // `canvas_h` used to store directly.
    let (cw_grid, ch_grid) = view.visible_cells();

    let x0 = view.scroll.0.floor() as i32 - 1;
    let y0 = view.scroll.1.floor() as i32 - 1;
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
            // Step 8-3: an animated tile shows its clip's current frame
            // (`anim_time`, the editor's own clock) first; then its sprite;
            // then its glyph — the same precedence play mode draws with.
            let image = tile
                .clip
                .as_ref()
                .and_then(|c| sprites.clip_frame(c, anim_time))
                .or_else(|| tile.sprite.as_ref().and_then(|s| sprites.resolve(s)));
            if let Some((tex, src)) = image {
                let (px, py) = view.grid_to_pixel(gx, gy);
                let (cw, ch) = view.cell_px();
                let dest = ember2d_sim::math::Rect::new(px as f32, py as f32, cw, ch);
                // Off-screen skip, same rule `draw_scaled_tile` applies.
                let vp = view.viewport;
                if dest.x + dest.w <= vp.x
                    || dest.x >= vp.x + vp.w
                    || dest.y + dest.h <= vp.y
                    || dest.y >= vp.y + vp.h
                {
                    continue;
                }
                // Inactive layers dim, the same as a glyph's colors do.
                let tint =
                    if lyr != active_layer { Color::Rgb(110, 110, 110) } else { Color::White };
                renderer.draw_texture_px(dest, tex, Some(src), tint);
                continue;
            }
            draw_scaled_tile(renderer, gx, gy, tile.glyph, fg, bg, view);
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
    renderer: &mut dyn DrawSurface,
    grid: &LevelGrid,
    view: &CanvasView,
) {
    // Visibility range in grid cells
    let (cw_grid, ch_grid) = view.visible_cells();

    let start_gx = view.scroll.0.floor() as i32;
    let start_gy = view.scroll.1.floor() as i32;

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
            draw_scaled_tile(
                renderer,
                gx,
                gy,
                ch,
                Color::DarkGrey,
                Color::Reset,
                view,
            );
        }
    }
}

pub fn draw_void(
    renderer: &mut dyn DrawSurface,
    grid: &LevelGrid,
    view: &CanvasView,
) {
    let (cw_grid, ch_grid) = view.visible_cells();

    let start_gx = view.scroll.0.floor() as i32;
    let start_gy = view.scroll.1.floor() as i32;

    for gy in start_gy..(start_gy + ch_grid + 1) {
        for gx in start_gx..(start_gx + cw_grid + 1) {
            if !grid.in_bounds(gx, gy) {
                draw_scaled_tile(
                    renderer,
                    gx,
                    gy,
                    ' ',
                    Color::Reset,
                    Color::Black,
                    view,
                );
            }
        }
    }
}

pub fn draw_level_boundary(
    renderer: &mut dyn DrawSurface,
    grid: &LevelGrid,
    view: &CanvasView,
) {
    let gw = grid.width as i32;
    let gh = grid.height as i32;
    for gy in 0..gh {
        draw_scaled_tile(
            renderer,
            gw,
            gy,
            '|',
            Color::DarkGrey,
            Color::Reset,
            view,
        );
    }
    for gx in 0..gw {
        draw_scaled_tile(
            renderer,
            gx,
            gh,
            '-',
            Color::DarkGrey,
            Color::Reset,
            view,
        );
    }
    draw_scaled_tile(renderer, gw, gh, '+', Color::DarkGrey, Color::Reset, view);
}

pub fn draw_cursor_highlight(
    renderer: &mut dyn DrawSurface,
    mouse: &ember2d::mouse::MouseState,
    palette: &crate::editor::palette::TilePalette,
    select_mode: bool,
    view: &CanvasView,
) {
    if !mouse.in_bounds || !view.viewport.contains(mouse.pixel_x, mouse.pixel_y) {
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
    // Step 9-5: both go through `CanvasView::pixel_to_grid` now.
    let (gx, gy) = view.pixel_to_grid(mouse.pixel_x, mouse.pixel_y);

    if select_mode {
        // Correct color for select mode as per plan: stark Dark Blue background for visibility
        draw_scaled_tile(
            renderer,
            gx,
            gy,
            '+',
            Color::Yellow,
            Color::DarkBlue,
            view,
        );
    } else {
        let tile = palette.current();
        // Correct color for paint mode as per plan: stark White background to make it pop
        draw_scaled_tile(
            renderer,
            gx,
            gy,
            tile.glyph,
            tile.fg,
            Color::White,
            view,
        );
    }
}

pub fn draw_spawn_marker(
    renderer: &mut dyn DrawSurface,
    spawn: (f32, f32),
    view: &CanvasView,
) {
    draw_scaled_tile(
        renderer,
        spawn.0.round() as i32,
        spawn.1.round() as i32,
        '@',
        Color::Green,
        Color::Reset,
        view,
    );
}

pub fn draw_extra_spawns(
    renderer: &mut dyn DrawSurface,
    spawns: &[(String, f32, f32)],
    view: &CanvasView,
) {
    // Cell-space canvas bounds, derived from `viewport` (pixels) — the
    // label below is drawn via `Renderer::draw_str`, which is still
    // cell-based, so this stays in cells rather than switching the bounds
    // check itself to `viewport.contains` (7C-3, master plan §5.3, E4).
    let vp = view.viewport;
    let canvas_x = (vp.x / CELL_W).round() as usize;
    let canvas_y = (vp.y / CELL_H).round() as usize;
    let canvas_w = (vp.w / CELL_W).round() as usize;
    let canvas_h = (vp.h / CELL_H).round() as usize;

    for (name, x, y) in spawns {
        let gx = x.round() as i32;
        let gy = y.round() as i32;
        draw_scaled_tile(
            renderer,
            gx,
            gy,
            '!',
            Color::Magenta,
            Color::Reset,
            view,
        );

        let (px, py) = view.grid_to_pixel(gx, gy);
        let label: String = name.chars().take(3).collect();
        // Clipping for label. The label sits one marker-WIDTH to the right
        // of the marker glyph, not a flat +1 character cell (Phase 7 Part
        // 1e, docs/ember2d-phase7-plan.md, E3) — the marker itself is drawn
        // `CELL_W * zoom` pixels wide (`draw_scaled_tile` scales it), so at
        // zoom 1.0 that's exactly one cell and the old flat `+1` happened to
        // match, but at any other zoom the marker's on-screen footprint no
        // longer lines up with a single character cell and the label ended
        // up overlapping it instead of sitting beside it.
        // Step 9-5: one grid cell is `cell_px().0` pixels wide, more than
        // one glyph cell when the world cell is wider than 8 px.
        let lx = ((px as f32 + view.cell_px().0) / CELL_W).round() as usize;
        let ly = (py as f32 / CELL_H).round() as usize;
        if lx >= canvas_x && lx < canvas_x + canvas_w && ly >= canvas_y && ly < canvas_y + canvas_h
        {
            renderer.draw_str(lx, ly, &label, Color::Magenta, Color::Reset);
        }
    }
}

pub fn draw_rect_preview(
    renderer: &mut dyn DrawSurface,
    anchor: (i32, i32),
    current: (i32, i32),
    glyph: char,
    view: &CanvasView,
) {
    let x0 = anchor.0.min(current.0);
    let y0 = anchor.1.min(current.1);
    let x1 = anchor.0.max(current.0);
    let y1 = anchor.1.max(current.1);
    for gy in y0..=y1 {
        for gx in x0..=x1 {
            draw_scaled_tile(
                renderer,
                gx,
                gy,
                glyph,
                Color::Black,
                Color::White,
                view,
            );
        }
    }
}

pub fn draw_line_preview(
    renderer: &mut dyn DrawSurface,
    anchor: (i32, i32),
    current: (i32, i32),
    glyph: char,
    view: &CanvasView,
) {
    for (gx, gy) in bresenham(anchor, current) {
        draw_scaled_tile(
            renderer,
            gx,
            gy,
            glyph,
            Color::Black,
            Color::Cyan,
            view,
        );
    }
}

pub fn draw_selection_preview(
    renderer: &mut dyn DrawSurface,
    anchor: (i32, i32),
    current: (i32, i32),
    view: &CanvasView,
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
                draw_scaled_tile(renderer, gx, gy, ch, fg, bg, view);
            } else if is_edge {
                let ch = if gx == x0 || gx == x1 { '│' } else { '─' };
                draw_scaled_tile(renderer, gx, gy, ch, fg, Color::Reset, view);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_paste_preview(
    renderer: &mut dyn DrawSurface,
    clipboard: &[(i32, i32, TileRecord)],
    cursor: (i32, i32),
    flip_x: bool,
    flip_y: bool,
    rotate: i32,
    view: &CanvasView,
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
            view,
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
    renderer: &mut dyn DrawSurface,
    grid: &LevelGrid,
    active_layer: u8,
    view: &CanvasView,
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
            draw_scaled_tile(renderer, gx, gy, tile.glyph, fg, bg, view);
        }
    }
}

pub fn draw_erase_preview(
    renderer: &mut dyn DrawSurface,
    grid_pos: (i32, i32),
    erase_size: usize,
    view: &CanvasView,
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
                view,
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
    fn pixel_to_grid(px: i32, py: i32, view: &CanvasView) -> (i32, i32) {
        view.pixel_to_grid(px as f32, py as f32)
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
            UiRect::new(0.0, 32.0, 640.0, 336.0),   // (0, 2, 80, 21) cells
            UiRect::new(112.0, 48.0, 400.0, 288.0), // (14, 3, 50, 18) cells — non-zero origin, e.g. a docked Hierarchy + Inspector layout
        ];
        for &viewport in &viewports {
            for &zoom in &[0.5f32, 1.0, 2.0, 3.0] {
                for &scroll in &[(0.0f32, 0.0f32), (5.0, 3.0), (2.5, 7.5), (100.0, 60.0)] {
                    for &(gx, gy) in &[(0i32, 0i32), (1, 1), (10, 4), (39, 19)] {
                        let view = CanvasView::new(scroll, zoom, viewport);
                        let (px, py) = view.grid_to_pixel(gx, gy);
                        let (gx2, gy2) = pixel_to_grid(px, py, &view);
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

    /// Step 9-5: a 16×16 world cell draws square — 16 logical pixels both
    /// ways at zoom 1, glyphs stretched (2, 1) — and still round-trips.
    #[test]
    fn a_square_world_cell_maps_grid_cells_to_square_pixels() {
        let mut view = CanvasView::new((0.0, 0.0), 1.0, UiRect::new(0.0, 0.0, 640.0, 320.0));
        view.cell = (16.0, 16.0);
        assert_eq!(view.grid_to_pixel(1, 1), (16, 16));
        assert_eq!(view.glyph_size(), [2.0, 1.0]);
        assert_eq!(view.visible_cells(), (40, 20));
        view.zoom = 2.0;
        view.scroll = (3.5, 1.0);
        for &(gx, gy) in &[(4i32, 1i32), (10, 7), (39, 19)] {
            let (px, py) = view.grid_to_pixel(gx, gy);
            assert_eq!(pixel_to_grid(px, py, &view), (gx, gy));
        }
    }
}
