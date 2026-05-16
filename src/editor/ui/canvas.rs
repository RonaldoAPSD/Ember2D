// editor/ui/canvas.rs — Drawing functions for the editor canvas.

use crate::renderer::{color::Color, Renderer};
use crate::editor::grid::LevelGrid;
use crate::level::TileRecord;
use super::types::Layout;

pub fn grid_to_pixel(gx: i32, gy: i32, scroll: (f32, f32), zoom: f32, layout: &Layout) -> (i32, i32) {
    let px = ((gx as f32 - scroll.0) * 8.0 * zoom).round() as i32;
    let py = ((gy as f32 - scroll.1) * 16.0 * zoom).round() as i32;
    (px + layout.canvas_x as i32 * 8, py + layout.canvas_y as i32 * 16)
}

pub fn draw_scaled_tile(renderer: &mut Renderer, gx: i32, gy: i32, glyph: char, fg: Color, bg: Color, scroll: (f32, f32), zoom: f32, layout: &Layout) {
    let (px, py) = grid_to_pixel(gx, gy, scroll, zoom, layout);
    
    // ── Performance Skip (Entirely off-screen check) ──────────────────────
    let cx = layout.canvas_x as i32 * 8;
    let cy = layout.canvas_y as i32 * 16;
    let cw = layout.canvas_w as i32 * 8;
    let ch = layout.canvas_h as i32 * 16;
    
    let tw = (8.0 * zoom).ceil() as i32;
    let th = (16.0 * zoom).ceil() as i32;
    
    // If the tile is ENTIRELY outside the viewport panel, skip it.
    // If it's partially inside, the hardware scissor will handle the clipping.
    if px + tw <= cx || px >= cx + cw || py + th <= cy || py >= cy + ch {
        return;
    }
    
    renderer.draw_char_scaled_pixels(px, py, glyph, fg, bg, zoom);
}

pub fn draw_grid(renderer: &mut Renderer, grid: &LevelGrid, active_layer: u8, scroll: (f32, f32), zoom: f32, layout: &Layout) {
    // Optimized range: only iterate tiles potentially on screen
    let cw_grid = (layout.canvas_w as f32 / zoom).ceil() as i32;
    let ch_grid = (layout.canvas_h as f32 / zoom).ceil() as i32;
    
    let x0 = scroll.0.floor() as i32 - 1;
    let y0 = scroll.1.floor() as i32 - 1;
    let x1 = x0 + cw_grid + 2;
    let y1 = y0 + ch_grid + 2;

    for l in 0..3 {
        for (&(gx, gy, lyr), tile) in &grid.tiles {
            if lyr != l { continue; }
            if gx < x0 || gx > x1 || gy < y0 || gy > y1 { continue; }
            
            let (mut fg, mut bg) = (tile.fg, tile.bg);
            if lyr != active_layer {
                fg = dim_color(fg);
                if bg != Color::Reset { bg = dim_color(bg); }
            }
            draw_scaled_tile(renderer, gx, gy, tile.glyph, fg, bg, scroll, zoom, layout);
        }
    }
}

fn dim_color(c: Color) -> Color {
    match c {
        Color::White   => Color::Grey,
        Color::Grey    => Color::DarkGrey,
        Color::Red     => Color::DarkRed,
        Color::Green   => Color::DarkGreen,
        Color::Blue    => Color::DarkBlue,
        Color::Yellow  => Color::DarkYellow,
        Color::Cyan    => Color::DarkCyan,
        Color::Magenta => Color::DarkMagenta,
        _              => Color::DarkGrey,
    }
}

pub fn draw_grid_overlay(renderer: &mut Renderer, grid: &LevelGrid, scroll: (f32, f32), zoom: f32, layout: &Layout) {
    // Visibility range in grid cells
    let cw_grid = (layout.canvas_w as f32 / zoom).ceil() as i32;
    let ch_grid = (layout.canvas_h as f32 / zoom).ceil() as i32;

    let start_gx = scroll.0.floor() as i32;
    let start_gy = scroll.1.floor() as i32;

    for gy in start_gy..(start_gy + ch_grid + 1) {
        for gx in start_gx..(start_gx + cw_grid + 1) {
            let on_col = gx % 5 == 0;
            let on_row = gy % 5 == 0;
            if !on_col && !on_row { continue; }
            if (0..3).any(|l| grid.get(gx, gy, l).is_some()) { continue; }
            let ch = if on_col && on_row { '+' } else { '.' };
            draw_scaled_tile(renderer, gx, gy, ch, Color::DarkGrey, Color::Reset, scroll, zoom, layout);
        }
    }
}

pub fn draw_void(renderer: &mut Renderer, grid: &LevelGrid, scroll: (f32, f32), zoom: f32, layout: &Layout) {
    let cw_grid = (layout.canvas_w as f32 / zoom).ceil() as i32;
    let ch_grid = (layout.canvas_h as f32 / zoom).ceil() as i32;

    let start_gx = scroll.0.floor() as i32;
    let start_gy = scroll.1.floor() as i32;

    for gy in start_gy..(start_gy + ch_grid + 1) {
        for gx in start_gx..(start_gx + cw_grid + 1) {
            if !grid.in_bounds(gx, gy) {
                draw_scaled_tile(renderer, gx, gy, ' ', Color::Reset, Color::Black, scroll, zoom, layout);
            }
        }
    }
}

pub fn draw_level_boundary(renderer: &mut Renderer, grid: &LevelGrid, scroll: (f32, f32), zoom: f32, layout: &Layout) {
    let gw = grid.width as i32;
    let gh = grid.height as i32;
    for gy in 0..gh {
        draw_scaled_tile(renderer, gw, gy, '|', Color::DarkGrey, Color::Reset, scroll, zoom, layout);
    }
    for gx in 0..gw {
        draw_scaled_tile(renderer, gx, gh, '-', Color::DarkGrey, Color::Reset, scroll, zoom, layout);
    }
    draw_scaled_tile(renderer, gw, gh, '+', Color::DarkGrey, Color::Reset, scroll, zoom, layout);
}

pub fn draw_cursor_highlight(renderer: &mut Renderer, mouse: &crate::mouse::MouseState, palette: &crate::editor::palette::TilePalette, select_mode: bool, zoom: f32, layout: &Layout) {
    if !mouse.in_bounds { return; }
    let col = mouse.cell_x;
    let row = mouse.cell_y;
    if col < layout.canvas_x || col >= layout.canvas_x + layout.canvas_w
        || row < layout.canvas_y || row >= layout.canvas_y + layout.canvas_h { return; }
    
    // Find grid cell under mouse (inverse of pixel math)
    let gx = (((col - layout.canvas_x) as f32) / zoom).floor() as i32;
    let gy = (((row - layout.canvas_y) as f32) / zoom).floor() as i32;
    
    if select_mode {
        draw_scaled_tile(renderer, gx, gy, '+', Color::Yellow, Color::DarkGrey, (0.0, 0.0), zoom, layout);
    } else {
        let tile = palette.current();
        draw_scaled_tile(renderer, gx, gy, tile.glyph, Color::Black, Color::White, (0.0, 0.0), zoom, layout);
    }
}

pub fn draw_spawn_marker(renderer: &mut Renderer, spawn: (f32, f32), scroll: (f32, f32), zoom: f32, layout: &Layout) {
    draw_scaled_tile(renderer, spawn.0.round() as i32, spawn.1.round() as i32, '@', Color::Green, Color::Reset, scroll, zoom, layout);
}

pub fn draw_extra_spawns(renderer: &mut Renderer, spawns: &[(String, f32, f32)], scroll: (f32, f32), zoom: f32, layout: &Layout) {
    for (name, x, y) in spawns {
        let gx = x.round() as i32;
        let gy = y.round() as i32;
        draw_scaled_tile(renderer, gx, gy, '!', Color::Magenta, Color::Reset, scroll, zoom, layout);
        
        let (px, py) = grid_to_pixel(gx, gy, scroll, zoom, layout);
        let label: String = name.chars().take(3).collect();
        // Clipping for label
        let lx = (px / 8) as usize + 1;
        let ly = (py / 16) as usize;
        if lx >= layout.canvas_x && lx < layout.canvas_x + layout.canvas_w && ly >= layout.canvas_y && ly < layout.canvas_y + layout.canvas_h {
            renderer.draw_str(lx, ly, &label, Color::Magenta, Color::Reset);
        }
    }
}

pub fn draw_rect_preview(renderer: &mut Renderer, anchor: (i32, i32), current: (i32, i32), glyph: char, scroll: (f32, f32), zoom: f32, layout: &Layout) {
    let x0 = anchor.0.min(current.0);
    let y0 = anchor.1.min(current.1);
    let x1 = anchor.0.max(current.0);
    let y1 = anchor.1.max(current.1);
    for gy in y0..=y1 {
        for gx in x0..=x1 {
            draw_scaled_tile(renderer, gx, gy, glyph, Color::Black, Color::White, scroll, zoom, layout);
        }
    }
}

pub fn draw_line_preview(renderer: &mut Renderer, anchor: (i32, i32), current: (i32, i32), glyph: char, scroll: (f32, f32), zoom: f32, layout: &Layout) {
    for (gx, gy) in bresenham(anchor, current) {
        draw_scaled_tile(renderer, gx, gy, glyph, Color::Black, Color::Cyan, scroll, zoom, layout);
    }
}

pub fn draw_selection_preview(renderer: &mut Renderer, anchor: (i32, i32), current: (i32, i32), scroll: (f32, f32), zoom: f32, layout: &Layout) {
    let x0 = anchor.0.min(current.0);
    let y0 = anchor.1.min(current.1);
    let x1 = anchor.0.max(current.0);
    let y1 = anchor.1.max(current.1);
    for gy in y0..=y1 {
        for gx in x0..=x1 {
            if gx == x0 || gx == x1 || gy == y0 || gy == y1 {
                draw_scaled_tile(renderer, gx, gy, '+', Color::Yellow, Color::DarkGrey, scroll, zoom, layout);
            }
        }
    }
}

pub fn draw_paste_preview(renderer: &mut Renderer, clipboard: &[(i32, i32, TileRecord)], cursor: (i32, i32), flip_x: bool, flip_y: bool, rotate: i32, scroll: (f32, f32), zoom: f32, layout: &Layout) {
    let max_dx = clipboard.iter().map(|(dx, _, _)| *dx).max().unwrap_or(0);
    let max_dy = clipboard.iter().map(|(_, dy, _)| *dy).max().unwrap_or(0);
    for (dx, dy, tile) in clipboard {
        let (tdx, tdy) = transform_offset(*dx, *dy, max_dx, max_dy, flip_x, flip_y, rotate);
        draw_scaled_tile(renderer, cursor.0 + tdx, cursor.1 + tdy, tile.glyph, Color::Black, Color::Yellow, scroll, zoom, layout);
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

pub fn draw_physics_overlay(renderer: &mut Renderer, grid: &LevelGrid, active_layer: u8, scroll: (f32, f32), zoom: f32, layout: &Layout) {
    for l in 0..3 {
        for (&(gx, gy, lyr), tile) in &grid.tiles {
            if lyr != l { continue; }
            let is_exit = tile.next_level.is_some();
            if !tile.solid && !tile.trigger && !is_exit { continue; }
            let (mut fg, mut bg) = if is_exit { (Color::White, Color::Cyan) }
                           else if tile.solid { (Color::White, Color::DarkRed) }
                           else { (Color::Black, Color::DarkYellow) };
            if lyr != active_layer {
                fg = dim_color(fg);
                bg = dim_color(bg);
            }
            draw_scaled_tile(renderer, gx, gy, tile.glyph, fg, bg, scroll, zoom, layout);
        }
    }
}

pub fn draw_erase_preview(renderer: &mut Renderer, grid_pos: (i32, i32), erase_size: usize, scroll: (f32, f32), zoom: f32, layout: &Layout) {
    if erase_size <= 1 { return; }
    let half = (erase_size as i32) / 2;
    for dy in -half..=half {
        for dx in -half..=half {
            draw_scaled_tile(renderer, grid_pos.0 + dx, grid_pos.1 + dy, 'X', Color::Red, Color::DarkRed, scroll, zoom, layout);
        }
    }
}
