// editor/start_screen/drawing.rs — UI rendering for the editor's start screen.

use ember2d::renderer::{color::Color, Renderer};
use ember2d::project::ProjectData;
use super::mod_types::*;

// ── Layout constants ─────────────────────────────────────────────────────────

pub const DEFAULT_SCR_W: usize = 80;
pub const DEFAULT_SCR_H: usize = 24;

const MENU_W: usize = 56;

const WIZ_W: usize = 62;
const WIZ_H: usize = 12;

const TBOX_W: usize = 70;
const TBOX_H: usize = 15;
const TCARD_W:   usize = 28;
const TCARD_GAP: usize = 4;
const TCARD_H:   usize = 8;

pub(super) const FB_W:       usize = 66;
pub(super) const FB_H:       usize = 18;

pub(super) const BROW_W: usize = 60;
pub(super) const BROW_H: usize = 16;

pub(super) fn draw_header(renderer: &mut Renderer, sw: usize) {
    renderer.draw_rect_filled(0, 0, sw, 2, ' ', Color::White, Color::DarkBlue);
    let title = "EMBER2D  LEVEL  EDITOR";
    renderer.draw_str(sw.saturating_sub(title.len()) / 2, 0, title, Color::White, Color::DarkBlue);
    let sub   = "Game Development Toolkit";
    renderer.draw_str(sw.saturating_sub(sub.len()) / 2, 1, sub, Color::Cyan, Color::DarkBlue);
}

fn draw_hint_bar(renderer: &mut Renderer, sw: usize, sh: usize, hint: &str) {
    renderer.draw_rect_filled(0, sh.saturating_sub(1), sw, 1, ' ', Color::White, Color::DarkGrey);
    let x = (sw.saturating_sub(hint.len())) / 2;
    renderer.draw_str(x, sh.saturating_sub(1), hint, Color::White, Color::DarkGrey);
}

pub(super) fn draw_main_menu(renderer: &mut Renderer, sw: usize, sh: usize, elapsed: f32, cursor: usize) {
    let box_w: usize = 44;
    let box_x = (sw.saturating_sub(box_w)) / 2;
    let box_y: usize = 3;
    let pulse = if (elapsed * 1.5) as u32 % 2 == 0 { Color::Cyan } else { Color::Yellow };
    renderer.draw_rect_outline(box_x, box_y, box_w, 5, pulse, Color::Black);
    renderer.draw_str(box_x + (box_w.saturating_sub(18)) / 2, box_y + 1, "* E M B E R  2 D *", Color::Yellow, Color::Black);
    renderer.draw_str(box_x + (box_w.saturating_sub(22)) / 2, box_y + 2, "L E V E L   E D I T O R", Color::White, Color::Black);
    renderer.draw_str(box_x + (box_w.saturating_sub(6)) / 2, box_y + 3, &format!("v{}", env!("CARGO_PKG_VERSION")), Color::DarkGrey, Color::Black);

    let menu_x = (sw.saturating_sub(MENU_W)) / 2;
    let menu_y = 10;
    let sep = "-".repeat(MENU_W);
    renderer.draw_str(menu_x, menu_y, &sep, Color::DarkGrey, Color::Black);
    renderer.draw_str(menu_x, menu_y + 2 + MENU_LABELS.len() * 3, &sep, Color::DarkGrey, Color::Black);

    for (i, &(label, desc)) in MENU_LABELS.iter().enumerate() {
        let row = menu_y + 2 + i * 3;
        if i == cursor {
            let line = format!("  >  {}. {:<44}", i + 1, label);
            renderer.draw_str(menu_x, row, &line, Color::Black, Color::Cyan);
            renderer.draw_str(menu_x, row + 1, &format!("{:width$}", "", width = MENU_W), Color::Black, Color::DarkBlue);
            renderer.draw_str(menu_x + 9, row + 1, desc, Color::Cyan, Color::DarkBlue);
        } else {
            renderer.draw_str(menu_x, row, &format!("     {}. {}", i + 1, label), Color::White, Color::Black);
            renderer.draw_str(menu_x + 9, row + 1, desc, Color::DarkGrey, Color::Black);
        }
    }
    draw_hint_bar(renderer, sw, sh, "Up/Down or hover: navigate  |  Enter or click: select");
}

pub(super) fn draw_text_step(renderer: &mut Renderer, sw: usize, sh: usize, step: usize, total: usize, step_name: &str, prompt: &str, info: &str, hint: &str, buffer: &str) {
    let wiz_x = (sw.saturating_sub(WIZ_W)) / 2;
    let wiz_y = 5;
    renderer.draw_rect_outline(wiz_x, wiz_y, WIZ_W, WIZ_H, Color::Cyan, Color::Black);
    renderer.draw_rect_filled(wiz_x + 1, wiz_y + 1, WIZ_W.saturating_sub(2), WIZ_H.saturating_sub(2), ' ', Color::White, Color::Black);
    renderer.draw_str(wiz_x + 1, wiz_y, &format!(" NEW PROJECT - Step {}/{}: {} ", step, total, step_name), Color::Black, Color::Cyan);
    renderer.draw_str(wiz_x + 3, wiz_y + 3, prompt, Color::White, Color::Black);
    let field_w = WIZ_W.saturating_sub(6);
    renderer.draw_rect_filled(wiz_x + 3, wiz_y + 5, field_w, 1, ' ', Color::Black, Color::White);
    let display = if buffer.len() + 3 >= field_w { format!("{}|", &buffer[buffer.len() + 3 - field_w..]) } else { format!("{}|", buffer) };
    renderer.draw_str(wiz_x + 4, wiz_y + 5, &display, Color::Black, Color::White);
    renderer.draw_str(wiz_x + 3, wiz_y + 8, info, Color::DarkGrey, Color::Black);
    draw_hint_bar(renderer, sw, sh, hint);
}

pub(super) fn draw_template_step(renderer: &mut Renderer, sw: usize, sh: usize, selected: usize) {
    let tbox_x = (sw.saturating_sub(TBOX_W)) / 2;
    let tbox_y = 4;
    let tcard_x0 = tbox_x + (TBOX_W.saturating_sub(TCARD_W * 2 + TCARD_GAP)) / 2;
    let tcard_y  = tbox_y + 4;

    renderer.draw_rect_outline(tbox_x, tbox_y, TBOX_W, TBOX_H, Color::Cyan, Color::Black);
    renderer.draw_rect_filled(tbox_x + 1, tbox_y + 1, TBOX_W.saturating_sub(2), TBOX_H.saturating_sub(2), ' ', Color::White, Color::Black);
    renderer.draw_str(tbox_x + 1, tbox_y, " NEW PROJECT - Step 5/5: Starting Template ", Color::Black, Color::Cyan);
    renderer.draw_str(tbox_x + 3, tbox_y + 2, "Choose how to start your first level:", Color::White, Color::Black);
    for (i, &(name, desc)) in TEMPLATE_LABELS.iter().enumerate() {
        let cx = tcard_x0 + i * (TCARD_W + TCARD_GAP);
        let cy = tcard_y;
        let is_sel = i == selected;
        let (border_c, hdr_bg) = if is_sel { (Color::Cyan, Color::Cyan) } else { (Color::DarkGrey, Color::DarkGrey) };
        renderer.draw_rect_outline(cx, cy, TCARD_W, TCARD_H, border_c, Color::Black);
        renderer.draw_rect_filled(cx + 1, cy + 1, TCARD_W.saturating_sub(2), 1, ' ', Color::Black, hdr_bg);
        renderer.draw_str(cx + 1, cy + 1, &format!(" {} {}", if is_sel { "[*]" } else { "[ ]" }, name), Color::Black, hdr_bg);
        let text_fg = if is_sel { Color::White } else { Color::Grey };
        let max_w = TCARD_W.saturating_sub(4);
        let mut lines = Vec::new(); let mut cur = String::new();
        for word in desc.split_whitespace() { if cur.is_empty() { cur = word.to_string(); } else if cur.len() + 1 + word.len() <= max_w { cur.push(' '); cur.push_str(word); } else { lines.push(cur.clone()); cur = word.to_string(); } }
        if !cur.is_empty() { lines.push(cur); }
        for (li, line) in lines.iter().take(4).enumerate() { renderer.draw_str(cx + 2, cy + 3 + li, line, text_fg, Color::Black); }
        if is_sel { renderer.draw_str(cx + 2, cy + TCARD_H.saturating_sub(2), "  click or Enter  ", Color::Black, Color::DarkGreen); }
    }
    draw_hint_bar(renderer, sw, sh, "Left/Right or hover: choose  |  Click or Enter: confirm  |  Esc: back");
}

pub(super) fn draw_style_step(renderer: &mut Renderer, sw: usize, sh: usize, selected: usize) {
    draw_card_wizard(renderer, sw, sh, 2, 5, "Visual Style", "Choose the visual aesthetic of your game:", STYLE_LABELS, selected);
}

pub(super) fn draw_loop_step(renderer: &mut Renderer, sw: usize, sh: usize, selected: usize) {
    draw_card_wizard(renderer, sw, sh, 3, 5, "Gameplay Loop", "Choose how your game world updates:", LOOP_LABELS, selected);
}

fn draw_card_wizard(renderer: &mut Renderer, sw: usize, sh: usize, step: usize, total: usize, step_name: &str, prompt: &str, labels: &[(&str, &str)], selected: usize) {
    let tbox_x = (sw.saturating_sub(TBOX_W)) / 2;
    let tbox_y = 4;
    let tcard_x0 = tbox_x + (TBOX_W.saturating_sub(TCARD_W * 2 + TCARD_GAP)) / 2;
    let tcard_y  = tbox_y + 4;

    renderer.draw_rect_outline(tbox_x, tbox_y, TBOX_W, TBOX_H, Color::Cyan, Color::Black);
    renderer.draw_rect_filled(tbox_x + 1, tbox_y + 1, TBOX_W.saturating_sub(2), TBOX_H.saturating_sub(2), ' ', Color::White, Color::Black);
    renderer.draw_str(tbox_x + 1, tbox_y, &format!(" NEW PROJECT - Step {}/{}: {} ", step, total, step_name), Color::Black, Color::Cyan);
    renderer.draw_str(tbox_x + 3, tbox_y + 2, prompt, Color::White, Color::Black);
    for (i, &(name, desc)) in labels.iter().enumerate() {
        let cx = tcard_x0 + i * (TCARD_W + TCARD_GAP);
        let cy = tcard_y;
        let is_sel = i == selected;
        let (border_c, hdr_bg) = if is_sel { (Color::Cyan, Color::Cyan) } else { (Color::DarkGrey, Color::DarkGrey) };
        renderer.draw_rect_outline(cx, cy, TCARD_W, TCARD_H, border_c, Color::Black);
        renderer.draw_rect_filled(cx + 1, cy + 1, TCARD_W.saturating_sub(2), 1, ' ', Color::Black, hdr_bg);
        renderer.draw_str(cx + 1, cy + 1, &format!(" {} {}", if is_sel { "[*]" } else { "[ ]" }, name), Color::Black, hdr_bg);
        let text_fg = if is_sel { Color::White } else { Color::Grey };
        let max_w = TCARD_W.saturating_sub(4);
        let mut lines = Vec::new(); let mut cur = String::new();
        for word in desc.split_whitespace() { if cur.is_empty() { cur = word.to_string(); } else if cur.len() + 1 + word.len() <= max_w { cur.push(' '); cur.push_str(word); } else { lines.push(cur.clone()); cur = word.to_string(); } }
        if !cur.is_empty() { lines.push(cur); }
        for (li, line) in lines.iter().take(4).enumerate() { renderer.draw_str(cx + 2, cy + 3 + li, line, text_fg, Color::Black); }
        if is_sel { renderer.draw_str(cx + 2, cy + TCARD_H.saturating_sub(2), "  click or Enter  ", Color::Black, Color::DarkGreen); }
    }
    draw_hint_bar(renderer, sw, sh, "Left/Right or hover: choose  |  Click or Enter: confirm  |  Esc: back");
}

pub(super) fn draw_folder_browser(renderer: &mut Renderer, sw: usize, sh: usize, fb_path: &std::path::PathBuf, fb_entries: &[String], fb_cursor: usize, project_folder: &str) {
    let fb_x = (sw.saturating_sub(FB_W)) / 2;
    let fb_y = 3;
    let fb_list_y = fb_y + 5;
    let fb_max_vis = FB_H.saturating_sub(7);

    renderer.draw_rect_outline(fb_x, fb_y, FB_W, FB_H, Color::Cyan, Color::Black);
    renderer.draw_rect_filled(fb_x + 1, fb_y + 1, FB_W.saturating_sub(2), FB_H.saturating_sub(2), ' ', Color::White, Color::Black);
    renderer.draw_str(fb_x + 1, fb_y, " NEW PROJECT - Step 4/5: Choose Location ", Color::Black, Color::Cyan);
    let path_str = fb_path.to_string_lossy(); let avail = FB_W.saturating_sub(14);
    let path_disp = if path_str.len() > avail { format!("...{}", &path_str[path_str.len().saturating_sub(avail - 3)..]) } else { path_str.to_string() };
    renderer.draw_str(fb_x + 2, fb_y + 1, "Location:", Color::DarkGrey, Color::Black);
    renderer.draw_str(fb_x + 12, fb_y + 1, &path_disp, Color::Cyan, Color::Black);
    let creates = fb_path.join(project_folder).to_string_lossy().to_string();
    let crt_disp = if creates.len() > avail { format!("...{}", &creates[creates.len().saturating_sub(avail - 3)..]) } else { creates };
    renderer.draw_str(fb_x + 2, fb_y + 2, "Creates: ", Color::DarkGrey, Color::Black);
    renderer.draw_str(fb_x + 12, fb_y + 2, &crt_disp, Color::Yellow, Color::Black);
    renderer.draw_str(fb_x + 2, fb_y + 3, &format!("{:-<width$}", "", width = FB_W.saturating_sub(4)), Color::DarkGrey, Color::Black);
    renderer.draw_str(fb_x + 2, fb_y + 4, "Subdirectories:", Color::DarkGrey, Color::Black);
    let offset = if fb_cursor >= fb_max_vis { fb_cursor - fb_max_vis + 1 } else { 0 };
    let item_w = FB_W.saturating_sub(4);
    for vis_i in 0..fb_max_vis {
        let list_i = offset + vis_i; if list_i >= fb_entries.len() { break; }
        let row = fb_list_y + vis_i; let is_sel = list_i == fb_cursor;
        let (label, text_fg, text_bg, sel_fg, sel_bg) = match fb_entries[list_i].as_str() {
            "\x00SELECT"     => ("[ Confirm: create project here ]", Color::DarkGreen, Color::Black, Color::Black, Color::DarkGreen),
            "\x00OS_BROWSER"  => ("[..] Open OS File Browser", Color::Cyan, Color::Black, Color::Black, Color::Cyan),
            "\x00NEW_FOLDER"  => ("[+] Create New Folder", Color::Yellow, Color::Black, Color::Black, Color::Yellow),
            "\x00PARENT"      => ("[..] Go up to parent folder", Color::Yellow, Color::Black, Color::Yellow, Color::DarkBlue),
            name => (name, Color::White, Color::Black, Color::White, Color::DarkBlue),
        };
        let padded = format!("{:<width$}", label, width = item_w.saturating_sub(2));
        let display = if padded.len() > item_w.saturating_sub(2) { &padded[..item_w.saturating_sub(2)] } else { &padded };
        if is_sel { renderer.draw_str(fb_x + 1, row, " >", sel_fg, sel_bg); renderer.draw_str(fb_x + 3, row, display, sel_fg, sel_bg); }
        else { renderer.draw_str(fb_x + 1, row, "  ", text_fg, text_bg); renderer.draw_str(fb_x + 3, row, display, text_fg, text_bg); }
    }
    if fb_entries.len() > fb_max_vis { let pct = fb_cursor * (FB_H.saturating_sub(8)) / fb_entries.len().max(1); renderer.draw_char(fb_x + FB_W - 1, fb_y + 5 + pct, '#', Color::DarkGrey, Color::Black); }
    draw_hint_bar(renderer, sw, sh, "Up/Down: navigate  |  Enter/click folder: open  |  Enter on Confirm: select  |  Esc: back");
}

pub(super) fn draw_browser(renderer: &mut Renderer, sw: usize, sh: usize, title: &str, items: &[String], cursor: usize, empty_msg: &str, empty_hint: &str, hint: &str, use_name_for: bool, fb_path: &std::path::PathBuf) {
    let brow_x = (sw.saturating_sub(BROW_W)) / 2;
    let brow_y = 4;
    let brow_list_y = brow_y + 4;
    let brow_max_vis = BROW_H.saturating_sub(5);

    renderer.draw_rect_outline(brow_x, brow_y, BROW_W, BROW_H, Color::Cyan, Color::Black);
    renderer.draw_rect_filled(brow_x + 1, brow_y + 1, BROW_W.saturating_sub(2), BROW_H.saturating_sub(2), ' ', Color::White, Color::Black);
    renderer.draw_str(brow_x + 1, brow_y, title, Color::Black, Color::Cyan);

    let path_str = fb_path.to_string_lossy(); let avail = BROW_W.saturating_sub(14);
    let path_disp = if path_str.len() > avail { format!("...{}", &path_str[path_str.len().saturating_sub(avail - 3)..]) } else { path_str.to_string() };
    renderer.draw_str(brow_x + 2, brow_y + 1, "Location:", Color::DarkGrey, Color::Black);
    renderer.draw_str(brow_x + 12, brow_y + 1, &path_disp, Color::Cyan, Color::Black);
    renderer.draw_str(brow_x + 2, brow_y + 2, &format!("{:-<width$}", "", width = BROW_W.saturating_sub(4)), Color::DarkGrey, Color::Black);

    if items.is_empty() {
        renderer.draw_str(brow_x + 3, brow_y + 6, empty_msg, Color::DarkGrey, Color::Black);
        if !empty_hint.is_empty() { renderer.draw_str(brow_x + 3, brow_y + 8, empty_hint, Color::DarkGrey, Color::Black); }
    } else {
        let offset = if cursor >= brow_max_vis { cursor - brow_max_vis + 1 } else { 0 };
        let item_w = BROW_W.saturating_sub(4);
        for (vis_i, list_i) in (0..brow_max_vis).zip(offset..) {
            if list_i >= items.len() { break; }
            let row = brow_list_y + vis_i; let raw = &items[list_i];
            
            let (label, fg, bg, sfg, sbg) = match raw.as_str() {
                "\x00OS_BROWSER" => ("[..] Open OS File Browser".to_string(), Color::Cyan, Color::Black, Color::Black, Color::Cyan),
                "\x00PARENT"     => ("[..] Go up to parent folder".to_string(), Color::Yellow, Color::Black, Color::Yellow, Color::DarkBlue),
                name => {
                    let has_ron = fb_path.join(name).join("project.ron").exists();
                    let disp = if has_ron && use_name_for { 
                        format!("* {}", ProjectData::name_for(&fb_path.join(name).to_string_lossy())) 
                    } else { 
                        // Extract filename only for display
                        std::path::Path::new(name)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| name.to_string())
                    };
                    (disp, Color::White, Color::Black, Color::White, Color::DarkBlue)
                }
            };

            let padded = format!("{:<width$}", label, width = item_w.saturating_sub(2));
            if list_i == cursor { renderer.draw_str(brow_x + 1, row, " >", sfg, sbg); renderer.draw_str(brow_x + 3, row, &padded, sfg, sbg); }
            else { renderer.draw_str(brow_x + 1, row, "  ", fg, bg); renderer.draw_str(brow_x + 3, row, &padded, fg, bg); }
        }
        if items.len() > brow_max_vis { let pct = (cursor * (BROW_H.saturating_sub(8))) / items.len().max(1); renderer.draw_char(brow_x + BROW_W - 1, brow_y + 4 + pct, '#', Color::DarkGrey, Color::Black); }
    }
    draw_hint_bar(renderer, sw, sh, hint);
}

pub(super) fn hit_test(mx: usize, my: usize, x: usize, y: usize, w: usize, h: usize) -> bool { mx >= x && mx < x + w && my >= y && my < y + h }

pub(super) fn menu_item_hit(sw: usize, mx: usize, my: usize, i: usize) -> bool {
    let menu_x = (sw.saturating_sub(MENU_W)) / 2;
    let menu_y = 10;
    hit_test(mx, my, menu_x, menu_y + 2 + i * 3, MENU_W, 2)
}

pub(super) fn folder_item_hit(sw: usize, mx: usize, my: usize, vis_i: usize) -> bool {
    let fb_x = (sw.saturating_sub(FB_W)) / 2;
    let fb_y = 3;
    let fb_list_y = fb_y + 5;
    hit_test(mx, my, fb_x, fb_list_y + vis_i, FB_W, 1)
}

pub(super) fn browser_item_hit(sw: usize, mx: usize, my: usize, vis_i: usize) -> bool {
    let brow_x = (sw.saturating_sub(BROW_W)) / 2;
    let brow_y = 4;
    let brow_list_y = brow_y + 4;
    hit_test(mx, my, brow_x, brow_list_y + vis_i, BROW_W, 1)
}

pub(super) fn template_item_hit(sw: usize, mx: usize, my: usize, i: usize) -> bool {
    let tbox_x = (sw.saturating_sub(TBOX_W)) / 2;
    let tbox_y = 4;
    let tcard_x0 = tbox_x + (TBOX_W.saturating_sub(TCARD_W * 2 + TCARD_GAP)) / 2;
    let tcard_y  = tbox_y + 4;
    let cx = tcard_x0 + i * (TCARD_W + TCARD_GAP);
    hit_test(mx, my, cx, tcard_y, TCARD_W, TCARD_H)
}
