// editor/ui/canvas.rs — Drawing functions for the editor canvas.

use crate::renderer::{color::Color, Renderer};
use crate::editor::grid::LevelGrid;
use crate::level::TileRecord;
use super::types::Layout;

pub fn grid_to_screen(gx: i32, gy: i32, scroll: (i32, i32), layout: &Layout) -> Option<(usize, usize)> {
    let sx = gx - scroll.0;
    let sy = gy - scroll.1;
    if sx < 0 || sy < 0 || sx >= layout.canvas_w as i32 || sy >= layout.canvas_h as i32 {
        return None;
    }
    Some((sx as usize + layout.canvas_x, sy as usize + layout.canvas_y))
}

pub fn draw_grid(renderer: &mut Renderer, grid: &LevelGrid, scroll: (i32, i32), layout: &Layout) {
    for ((gx, gy), tile) in grid.iter() {
        if let Some((col, row)) = grid_to_screen(*gx, *gy, scroll, layout) {
            renderer.draw_char(col, row, tile.glyph, tile.fg, tile.bg);
        }
    }
}

pub fn draw_grid_overlay(renderer: &mut Renderer, grid: &LevelGrid, scroll: (i32, i32), layout: &Layout) {
    for sy in 0..layout.canvas_h {
        for sx in 0..layout.canvas_w {
            let gx = sx as i32 + scroll.0;
            let gy = sy as i32 + scroll.1;
            let on_col = gx % 5 == 0;
            let on_row = gy % 5 == 0;
            if !on_col && !on_row { continue; }
            if grid.get(gx, gy).is_some() { continue; }
            let ch = if on_col && on_row { '+' } else { '.' };
            renderer.draw_char(sx + layout.canvas_x, sy + layout.canvas_y, ch, Color::DarkGrey, Color::Reset);
        }
    }
}

pub fn draw_void(renderer: &mut Renderer, grid: &LevelGrid, scroll: (i32, i32), layout: &Layout) {
    for sy in 0..layout.canvas_h {
        let row = sy + layout.canvas_y;
        if row >= renderer.height { break; }
        for sx in 0..layout.canvas_w {
            let col = sx + layout.canvas_x;
            if col >= renderer.width { break; }
            let gx = sx as i32 + scroll.0;
            let gy = sy as i32 + scroll.1;
            if !grid.in_bounds(gx, gy) {
                renderer.draw_char(col, row, ' ', Color::Reset, Color::Black);
            }
        }
    }
}

pub fn draw_level_boundary(renderer: &mut Renderer, grid: &LevelGrid, scroll: (i32, i32), layout: &Layout) {
    let right  = grid.width  as i32 - scroll.0;
    let bottom = grid.height as i32 - scroll.1;

    if right > 0 && right < layout.canvas_w as i32 {
        let col = right as usize + layout.canvas_x;
        for sy in 0..layout.canvas_h {
            renderer.draw_char(col, sy + layout.canvas_y, '|', Color::DarkGrey, Color::Reset);
        }
    }

    if bottom > 0 && bottom < layout.canvas_h as i32 {
        let row = bottom as usize;
        for sx in 0..layout.canvas_w {
            renderer.draw_char(sx + layout.canvas_x, row + layout.canvas_y, '-', Color::DarkGrey, Color::Reset);
        }
        if right > 0 && right < layout.canvas_w as i32 {
            renderer.draw_char(right as usize + layout.canvas_x, bottom as usize + layout.canvas_y, '+', Color::DarkGrey, Color::Reset);
        }
    }
}

pub fn draw_cursor_highlight(renderer: &mut Renderer, mouse: &crate::mouse::MouseState, palette: &crate::editor::palette::TilePalette, select_mode: bool, layout: &Layout) {
    if !mouse.in_bounds { return; }
    let col = mouse.cell_x;
    let row = mouse.cell_y;
    if col < layout.canvas_x || col >= layout.canvas_x + layout.canvas_w
        || row < layout.canvas_y || row >= layout.canvas_y + layout.canvas_h { return; }
    if select_mode {
        renderer.draw_char(col, row, '+', Color::Yellow, Color::DarkGrey);
    } else {
        let tile = palette.current();
        renderer.draw_char(col, row, tile.glyph, Color::Black, Color::White);
    }
}

pub fn draw_spawn_marker(renderer: &mut Renderer, spawn: (f32, f32), scroll: (i32, i32), layout: &Layout) {
    let gx = spawn.0.round() as i32;
    let gy = spawn.1.round() as i32;
    if let Some((col, row)) = grid_to_screen(gx, gy, scroll, layout) {
        renderer.draw_char(col, row, '@', Color::Green, Color::Reset);
    }
}

pub fn draw_extra_spawns(renderer: &mut Renderer, spawns: &[(String, f32, f32)], scroll: (i32, i32), layout: &Layout) {
    for (name, x, y) in spawns {
        let gx = x.round() as i32;
        let gy = y.round() as i32;
        if let Some((col, row)) = grid_to_screen(gx, gy, scroll, layout) {
            renderer.draw_char(col, row, '!', Color::Magenta, Color::Reset);
            let label: String = name.chars().take(3).collect();
            if col + 1 + label.len() < layout.canvas_w {
                renderer.draw_str(col + 1, row, &label, Color::Magenta, Color::Reset);
            }
        }
    }
}

pub fn draw_rect_preview(renderer: &mut Renderer, anchor: (i32, i32), current: (i32, i32), glyph: char, scroll: (i32, i32), layout: &Layout) {
    let x0 = anchor.0.min(current.0);
    let y0 = anchor.1.min(current.1);
    let x1 = anchor.0.max(current.0);
    let y1 = anchor.1.max(current.1);

    for gy in y0..=y1 {
        for gx in x0..=x1 {
            if let Some((col, row)) = grid_to_screen(gx, gy, scroll, layout) {
                renderer.draw_char(col, row, glyph, Color::Black, Color::White);
            }
        }
    }
}

pub fn draw_line_preview(renderer: &mut Renderer, anchor: (i32, i32), current: (i32, i32), glyph: char, scroll: (i32, i32), layout: &Layout) {
    for (gx, gy) in bresenham(anchor, current) {
        if let Some((col, row)) = grid_to_screen(gx, gy, scroll, layout) {
            renderer.draw_char(col, row, glyph, Color::Black, Color::Cyan);
        }
    }
}

pub fn draw_selection_preview(renderer: &mut Renderer, anchor: (i32, i32), current: (i32, i32), scroll: (i32, i32), layout: &Layout) {
    let x0 = anchor.0.min(current.0);
    let y0 = anchor.1.min(current.1);
    let x1 = anchor.0.max(current.0);
    let y1 = anchor.1.max(current.1);

    for gy in y0..=y1 {
        for gx in x0..=x1 {
            let on_edge = gx == x0 || gx == x1 || gy == y0 || gy == y1;
            if !on_edge { continue; }
            if let Some((col, row)) = grid_to_screen(gx, gy, scroll, layout) {
                renderer.draw_char(col, row, '+', Color::Yellow, Color::DarkGrey);
            }
        }
    }
}

pub fn draw_paste_preview(renderer: &mut Renderer, clipboard: &[(i32, i32, TileRecord)], cursor: (i32, i32), flip_x: bool, flip_y: bool, rotate: i32, scroll: (i32, i32), layout: &Layout) {
    let max_dx = clipboard.iter().map(|(dx, _, _)| *dx).max().unwrap_or(0);
    let max_dy = clipboard.iter().map(|(_, dy, _)| *dy).max().unwrap_or(0);

    for (dx, dy, tile) in clipboard {
        let (tdx, tdy) = transform_offset(*dx, *dy, max_dx, max_dy, flip_x, flip_y, rotate);
        let gx = cursor.0 + tdx;
        let gy = cursor.1 + tdy;
        if let Some((col, row)) = grid_to_screen(gx, gy, scroll, layout) {
            renderer.draw_char(col, row, tile.glyph, Color::Black, Color::Yellow);
        }
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
        if x0 == x1 && y0 == y1 { break; }
        let e2 = 2 * err;
        if e2 > -dy { err -= dy; x0 += sx; }
        if e2 <  dx { err += dx; y0 += sy; }
    }
    points
}

pub fn transform_offset(dx: i32, dy: i32, max_dx: i32, max_dy: i32, flip_x: bool, flip_y: bool, rotate: i32) -> (i32, i32) {
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

pub fn draw_physics_overlay(renderer: &mut Renderer, grid: &LevelGrid, scroll: (i32, i32), layout: &Layout) {
    for ((gx, gy), tile) in grid.iter() {
        let is_exit = tile.next_level.is_some();
        if !tile.solid && !tile.trigger && !is_exit { continue; }
        if let Some((col, row)) = grid_to_screen(*gx, *gy, scroll, layout) {
            if is_exit {
                renderer.draw_char(col, row, tile.glyph, Color::White, Color::Cyan);
            } else if tile.solid {
                renderer.draw_char(col, row, tile.glyph, Color::White, Color::DarkRed);
            } else {
                renderer.draw_char(col, row, tile.glyph, Color::Black, Color::DarkYellow);
            }
        }
    }
}

pub fn draw_erase_preview(renderer: &mut Renderer, grid_pos: (i32, i32), erase_size: usize, scroll: (i32, i32), layout: &Layout) {
    if erase_size <= 1 { return; }
    let half = (erase_size as i32) / 2;
    let (gx, gy) = grid_pos;
    for dy in -half..=half {
        for dx in -half..=half {
            if let Some((col, row)) = grid_to_screen(gx + dx, gy + dy, scroll, layout) {
                renderer.draw_char(col, row, 'X', Color::Red, Color::DarkRed);
            }
        }
    }
}
