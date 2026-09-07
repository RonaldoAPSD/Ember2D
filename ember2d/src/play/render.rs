// play/render.rs — Draw-list assembly and rendering support for PlayState.
//
// Split out of play.rs (via the sibling-directory submodule convention
// already used for `spawn.rs`/`tests.rs`) once play.rs crossed the
// project's 600-line hard limit — see CLAUDE.md "Development Rules". Pure
// module-file split: nothing here changed behavior, only location.
//
// Step 2d of docs/ember2d-refactor-plan.md: everything the entity draw loop
// used to build inline (an ad hoc tuple, sorted only by (z, id) for defect
// D5's sake) is now a real DrawCommand/DrawList, sorted by (space, z,
// texture, id). The texture dimension is the point of this step: WgpuBackend
// only merges *consecutive* same-texture instances into one draw call
// (`ensure_batch`), so a list that happened to interleave glyphs and
// textures degenerated into one draw call per sprite. Sorting by texture
// before submission means every sprite sharing a texture (including the
// font atlas, which every glyph implicitly shares) lands adjacent.
//
// `layer` from the plan's (space, layer, z, texture) isn't included yet —
// `Sprite` has no field distinct from its own `layer` to sort by, so
// there's nothing to add without inventing data that doesn't exist.
// `Sprite.layer` already folds in the tile's authored editor layer
// (`tile.layer as i32 * 10`, see play/spawn.rs — Phase 4 dropped the
// per-tag sub-ordering `z_for_tag` used to add), so this isn't a
// functional gap, just a naming one Phase 3's sprite model resolved.
//
// Step 2e: commands now carry a real world-space position instead of a
// pre-subtracted screen col/row — `Space::World` was a lie otherwise
// (screen coordinates labeled "World"). The camera conversion happens once,
// in `render`, via `Camera::world_to_screen`.

use ember2d_sim::components::SpriteSource;
use ember2d_sim::math::Vec2;
use crate::renderer::color::Color;
use crate::renderer::Renderer;
use ember2d_sim::world::{EntityId, World};
use ember2d_sim::scripting::{HudDraw, LogEntry, LogLevel, ShakeState};
use rand::Rng;
use rand::rngs::SmallRng;

/// World vs. screen space — every command built today is `World`; `Screen`
/// exists so HUD/particle work in later phases has somewhere to go without
/// another format change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Space { World, Screen }

pub struct DrawCommand<'w> {
    pub space: Space,
    pub z: i32,
    /// Sort tiebreak only, not display data — see defect D5's rationale.
    pub id: EntityId,
    pub world_pos: Vec2,
    pub source: &'w SpriteSource,
    pub tint: Color,
    pub size: Option<Vec2>,
}

/// The texture path a command should batch by, or `None` for anything that
/// isn't texture-sourced (glyphs share the font atlas implicitly; clips
/// aren't resolved to a texture at this layer at all).
fn texture_sort_key(source: &SpriteSource) -> Option<&str> {
    match source {
        SpriteSource::Texture { path, .. } => Some(path.as_str()),
        _ => None,
    }
}

pub struct DrawList<'w> {
    pub commands: Vec<DrawCommand<'w>>,
}

impl<'w> DrawList<'w> {
    /// Collect every visible sprite, sorted for rendering. Free
    /// function-shaped (an associated fn with no `&self`) so it's testable
    /// without a live GPU-backed `Renderer`. No camera involved here at
    /// all — that conversion happens per-command in `render`.
    pub(super) fn from_world(world: &'w World) -> Self {
        let mut commands: Vec<DrawCommand<'w>> = world.transforms.keys().filter_map(|&id| {
            let pos = world.get_global_position(id);
            world.sprites.get(&id).and_then(|sp| {
                if !sp.visible { return None; }
                Some(DrawCommand {
                    space: Space::World, z: sp.layer, id, world_pos: pos,
                    source: &sp.source, tint: sp.tint, size: sp.size,
                })
            })
        }).collect();

        commands.sort_unstable_by_key(|c| (c.space, c.z, texture_sort_key(c.source), c.id));
        DrawList { commands }
    }
}

/// A texture sprite's world-space size: `size` if explicit, else natural
/// size — the texture's pixel dimensions divided by `pixels_per_unit`
/// (Step 3b; replaces the old hardcoded `* 4.0` magic scale). Free function
/// so it's testable without a live GPU-backed `Renderer` or `AssetManager`.
pub(super) fn sprite_size(size: Option<Vec2>, texture_width: u32, texture_height: u32, pixels_per_unit: f32) -> Vec2 {
    size.unwrap_or_else(|| Vec2::new(
        texture_width as f32 / pixels_per_unit,
        texture_height as f32 / pixels_per_unit,
    ))
}

/// True if a screen cell at (col, row) falls inside the playable viewport —
/// i.e. on screen and above the bottom HUD bar (the last row is reserved).
///
/// Shared by both the glyph and texture draw paths in `render` (defect D13:
/// the texture path used to skip this check entirely, since it `continue`d
/// before the bounds test ran).
pub(super) fn in_viewport(col: i32, row: i32, width: usize, height: usize) -> bool {
    col >= 0 && row >= 0 && (col as usize) < width && (row as usize) < height.saturating_sub(1)
}

/// This frame's camera-shake offset, or zero if inactive — pulled out of
/// `play.rs`'s `render` (R15, 7A-5, docs/ember2d-master-plan.md) so it's
/// directly testable without a real `Renderer`, and to keep play.rs under
/// CLAUDE.md's 600-line limit (same reasoning this file's own header
/// comment gives). `render_rng` must be `PlayState::render_rng`, never
/// `rng` — see that field's own doc comment for why the two must stay
/// independent streams.
pub(super) fn camera_shake_jitter(render_rng: &mut SmallRng, shake_state: Option<ShakeState>, shake_timer: f32) -> Vec2 {
    match shake_state.filter(|s| s.duration > 0.0) {
        Some(shake) => {
            let intensity = shake.intensity * (shake_timer / shake.duration);
            Vec2::new(render_rng.gen_range(-intensity..=intensity), render_rng.gen_range(-intensity..=intensity))
        }
        None => Vec2::ZERO,
    }
}

// R15's fix (above) pushed play.rs over CLAUDE.md's 600-line limit — the
// two functions below are a small, mechanical slice of 7A-8's own planned
// "move HUD dispatch + debug overlay into play/hud.rs" pulled forward to
// close that gap now, by user direction, rather than leave play.rs worse
// than its already-tracked (R38) pre-existing overage. Pure relocation,
// same as everything else already split into this file — nothing here
// changed behavior.

/// The F3 debug overlay: level name, camera position, backend, FPS — see
/// `PlayState::show_debug`'s own doc comment (play.rs) for why this is a
/// toggle, not permanent chrome.
pub(super) fn draw_debug_overlay(renderer: &mut Renderer, level_name: &str, camera_pos: Vec2, fps: f32) {
    renderer.draw_rect_filled(0, 0, renderer.width, 1, ' ', Color::Black, Color::DarkBlue);
    renderer.draw_str(0, 0, &format!(" DEBUG: {}", level_name), Color::White, Color::DarkBlue);
    renderer.draw_str(38, 0, &format!("x:{:.1} y:{:.1}", camera_pos.x, camera_pos.y), Color::Green, Color::DarkBlue);
    renderer.draw_str(renderer.width.saturating_sub(18), 0, &format!("Mode:{}", renderer.backend_name()), Color::Cyan, Color::DarkBlue);
    renderer.draw_str(renderer.width.saturating_sub(6), 0, &format!("FPS:{}", fps.round()), Color::White, Color::DarkBlue);
}

/// Draws whatever a script queued via `ctx.draw_hud`/`draw_menu`/etc. this
/// frame — `Simulation::pending_hud_draws`'s own doc comment covers the
/// queue's lifecycle; this is purely the dispatch-by-variant drawing.
pub(super) fn draw_hud_queue<'a>(renderer: &mut Renderer, draws: impl Iterator<Item = &'a HudDraw>) {
    for hud in draws {
        match hud {
            HudDraw::Text { x, y, text, fg, bg } => if *x < renderer.width && *y < renderer.height { renderer.draw_str(*x, *y, text, *fg, *bg); }
            HudDraw::Box { x, y, w, h, fg, bg } => renderer.draw_rect_outline(*x, *y, *w, *h, *fg, *bg),
            HudDraw::Fill { x, y, w, h, ch, fg, bg } => renderer.draw_rect_filled(*x, *y, *w, *h, *ch, *fg, *bg),
            HudDraw::Menu { x, y, w, options, selected, fg, bg, sel_fg, sel_bg } =>
                crate::ui::Menu::new(*x, *y, *w, options.clone(), *selected).with_colors(*fg, *bg, *sel_fg, *sel_bg).draw(renderer),
            HudDraw::Panel { x, y, w, h, title, fg, bg } =>
                crate::ui::Panel::new(*x, *y, *w, *h).with_title(title).with_colors(*fg, *bg).draw(renderer),
        }
    }
}

/// The last `max` console log entries, newest at the bottom — used to sit
/// just above the old hardcoded HUD bar (Step 4g removed it; there's no
/// bar to sit above anymore, just the bottom of the full-height viewport).
pub(super) fn draw_recent_log(renderer: &mut Renderer, log: &[LogEntry], max: usize) {
    let log_len = log.len();
    for i in 0..log_len.min(max) {
        let entry = &log[log_len - 1 - i];
        let col = match entry.level {
            LogLevel::Error => Color::Red,
            LogLevel::Warning => Color::Yellow,
            LogLevel::Info => Color::Cyan,
        };
        renderer.draw_str(1, renderer.height - 1 - i, &entry.text, col, Color::Reset);
    }
}
