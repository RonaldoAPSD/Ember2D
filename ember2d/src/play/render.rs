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

use crate::renderer::color::Color;
use crate::renderer::Renderer;
use ember2d_sim::components::{AnimationClip, ClipFrames, SpriteSource};
use ember2d_sim::math::{Rect, Vec2};
use ember2d_sim::scripting::{HudDraw, LogEntry, LogLevel, ShakeState};
use ember2d_sim::world::{EntityId, World};
use rand::rngs::SmallRng;
use rand::Rng;

/// World vs. screen space — every command built today is `World`; `Screen`
/// exists so HUD/particle work in later phases has somewhere to go without
/// another format change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Space {
    World,
    Screen,
}

pub struct DrawCommand<'w> {
    pub space: Space,
    pub z: i32,
    /// Sort tiebreak only, not display data — see defect D5's rationale.
    pub id: EntityId,
    pub world_pos: Vec2,
    pub source: &'w SpriteSource,
    pub tint: Color,
    pub size: Option<Vec2>,
    /// Step 9-7: mirror the image (`Sprite::flip_x`/`flip_y`).
    pub flip: (bool, bool),
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
    /// all — that conversion happens per-command in `render`. Every
    /// tilemap cell is included (`from_world_in` with no window).
    #[cfg(test)]
    pub(super) fn from_world(world: &'w World) -> Self {
        Self::from_world_in(world, None)
    }

    /// `from_world`, plus Step 8-1's tilemap cells (docs/ember2d-master-
    /// plan.md §5.7) — only those inside `view`, a world-space rect (the
    /// camera's visible area; `None` = every cell). That window is the
    /// point: entity sprites are still all built then culled one by one in
    /// `render`, but a 200×200 map's 40,000 static cells now cost only the
    /// few thousand on screen, walked straight off the grid
    /// (`Tilemap::visible_cells`), before anything is sorted. Each cell
    /// becomes an ordinary `DrawCommand` at its layer's z (`TileLayer::z`,
    /// the same `layer * 10` a tile entity's sprite had) with the tilemap's
    /// entity id as its tiebreak, so it sorts into exactly the slot its
    /// old entity did and batches with every other glyph on the atlas.
    pub(super) fn from_world_in(world: &'w World, view: Option<Rect>) -> Self {
        let mut commands: Vec<DrawCommand<'w>> = world
            .transforms
            .keys()
            .filter_map(|&id| {
                let pos = world.get_global_position(id);
                world.sprites.get(&id).and_then(|sp| {
                    if !sp.visible {
                        return None;
                    }
                    Some(DrawCommand {
                        space: Space::World,
                        z: sp.layer,
                        id,
                        world_pos: pos,
                        source: &sp.source,
                        tint: sp.tint,
                        size: sp.size,
                        flip: (sp.flip_x, sp.flip_y),
                    })
                })
            })
            .collect();

        for (&map_id, map) in &world.tilemaps {
            for (z, x, y, source, tint, sprite_cell) in map.visible_cells(view) {
                commands.push(DrawCommand {
                    space: Space::World,
                    z,
                    id: map_id,
                    world_pos: Vec2::new(x as f32, y as f32),
                    source,
                    tint,
                    // Step 8-2: a tileset-region cell fills exactly one
                    // cell, as its glyph did — not its natural size.
                    size: if sprite_cell { Some(Vec2::new(1.0, 1.0)) } else { None },
                    flip: (false, false),
                });
            }
        }

        // Step 9-7: with `World::y_sort` on, a sprite's bottom edge orders
        // it within its layer — lower on screen draws later, in front.
        // Quantised to 1/64 of a cell so the key stays an integer.
        let y_key = |c: &DrawCommand| -> i64 {
            if !world.y_sort {
                return 0;
            }
            let h = c.size.map(|s| s.y).unwrap_or(1.0);
            ((c.world_pos.y + h) * 64.0).round() as i64
        };
        commands.sort_by_key(|c| (c.space, c.z, y_key(c), texture_sort_key(c.source), c.id));
        DrawList { commands }
    }
}

/// One frame of an animation clip, ready to draw: a glyph, or a sub-rect of
/// a texture. Step 8-3 (docs/ember2d-master-plan.md §5.7) — play mode only
/// ever drew `ClipFrames::Glyphs` clips (Step 3c's scope); a project clip
/// built in the editor is `ClipFrames::Rects`.
pub(super) enum ClipFrame<'c> {
    Glyph(char),
    Rect(&'c str, Rect),
}

/// Frame `frame` of `clip` (wrapped into range), or `None` for a clip with
/// no frames. A free function so the frame choice is testable without a
/// live `Renderer`.
pub(super) fn clip_frame(clip: &AnimationClip, frame: usize) -> Option<ClipFrame<'_>> {
    match &clip.frames {
        ClipFrames::Glyphs { frames } if !frames.is_empty() => {
            Some(ClipFrame::Glyph(frames[frame % frames.len()]))
        }
        ClipFrames::Rects { texture, frames } if !frames.is_empty() => {
            Some(ClipFrame::Rect(texture, frames[frame % frames.len()]))
        }
        _ => None,
    }
}

/// A texture sprite's world-space size: `size` if explicit, else natural
/// size — the pixel dimensions of what's actually drawn, divided by
/// `pixels_per_unit` (Step 3b; replaces the old hardcoded `* 4.0` magic
/// scale). "What's drawn" is the `src` sub-rect when there is one: Step
/// 8-2 found this used the WHOLE texture's size even then, so a single
/// sprite-sheet cell with no explicit size would have rendered as big as
/// the entire sheet. Free function so it's testable without a live
/// GPU-backed `Renderer` or `AssetManager`.
pub(super) fn sprite_size(
    size: Option<Vec2>,
    src: Option<Rect>,
    texture_width: u32,
    texture_height: u32,
    pixels_per_unit: f32,
) -> Vec2 {
    size.unwrap_or_else(|| {
        let (w, h) = match src {
            Some(r) => (r.w, r.h),
            None => (texture_width as f32, texture_height as f32),
        };
        Vec2::new(w / pixels_per_unit, h / pixels_per_unit)
    })
}

/// True if a screen cell at (col, row) falls inside the playable viewport.
///
/// Shared by both the glyph and texture draw paths in `render` (defect D13:
/// the texture path used to skip this check entirely, since it `continue`d
/// before the bounds test ran).
///
/// R28 (7B-4, docs/ember2d-master-plan.md §5.2/§3): this used to also
/// reject `row == height - 1` via a trailing `.saturating_sub(1)` on
/// `height`, reserving a bottom HUD bar row that Phase 4 removed — nothing
/// draws a HUD there anymore, so the reservation just silently culled the
/// bottom row of every level's playable viewport instead.
/// Step 9-7: the source rect to sample for a (possibly) flipped image —
/// mirrored by starting at the far edge with a negative extent, which the
/// shader's `uv_offset + uv * uv_size` turns into a reversed read. An
/// unflipped draw keeps its `src` as it was (`None` = the whole image).
pub(super) fn flipped_src(
    src: Option<Rect>,
    tex_w: u32,
    tex_h: u32,
    flip: (bool, bool),
) -> Option<Rect> {
    if flip == (false, false) {
        return src;
    }
    let mut r = src.unwrap_or_else(|| Rect::new(0.0, 0.0, tex_w as f32, tex_h as f32));
    if flip.0 {
        r = Rect::new(r.x + r.w, r.y, -r.w, r.h);
    }
    if flip.1 {
        r = Rect::new(r.x, r.y + r.h, r.w, -r.h);
    }
    Some(r)
}

pub(super) fn in_viewport(col: i32, row: i32, width: usize, height: usize) -> bool {
    col >= 0 && row >= 0 && (col as usize) < width && (row as usize) < height
}

/// Step 9-5 (docs/ember2d-master-plan.md §5.8): how many extra glyph cells
/// a world cell can hang off the left/top edge while still showing part of
/// itself — `cells_per_unit` rounded up, less the one cell `in_viewport`
/// already allows. (0, 0) on the classic grid at zoom 1, so ASCII culling
/// is exactly what it was.
pub(super) fn cull_slack(cells_per_unit: Vec2) -> (i32, i32) {
    let s = |v: f32| (v.ceil() as i32 - 1).max(0);
    (s(cells_per_unit.x), s(cells_per_unit.y))
}

/// This frame's camera-shake offset, or zero if inactive — pulled out of
/// `play.rs`'s `render` (R15, 7A-5, docs/ember2d-master-plan.md) so it's
/// directly testable without a real `Renderer`, and to keep play.rs under
/// CLAUDE.md's 600-line limit (same reasoning this file's own header
/// comment gives). `render_rng` must be `PlayState::render_rng`, never
/// `rng` — see that field's own doc comment for why the two must stay
/// independent streams.
pub(super) fn camera_shake_jitter(
    render_rng: &mut SmallRng,
    shake_state: Option<ShakeState>,
    shake_timer: f32,
) -> Vec2 {
    match shake_state.filter(|s| s.duration > 0.0) {
        Some(shake) => {
            let intensity = shake.intensity * (shake_timer / shake.duration);
            Vec2::new(
                render_rng.gen_range(-intensity..=intensity),
                render_rng.gen_range(-intensity..=intensity),
            )
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
pub(super) fn draw_debug_overlay(
    renderer: &mut Renderer,
    level_name: &str,
    camera_pos: Vec2,
    fps: f32,
) {
    renderer.draw_rect_filled(0, 0, renderer.width, 1, ' ', Color::Black, Color::DarkBlue);
    renderer.draw_str(0, 0, &format!(" DEBUG: {}", level_name), Color::White, Color::DarkBlue);
    renderer.draw_str(
        38,
        0,
        &format!("x:{:.1} y:{:.1}", camera_pos.x, camera_pos.y),
        Color::Green,
        Color::DarkBlue,
    );
    renderer.draw_str(
        renderer.width.saturating_sub(18),
        0,
        &format!("Mode:{}", renderer.backend_name()),
        Color::Cyan,
        Color::DarkBlue,
    );
    renderer.draw_str(
        renderer.width.saturating_sub(6),
        0,
        &format!("FPS:{}", fps.round()),
        Color::White,
        Color::DarkBlue,
    );
}

/// Draws whatever a script queued via `ctx.draw_hud`/`draw_menu`/etc. this
/// frame — `Simulation::pending_hud_draws`'s own doc comment covers the
/// queue's lifecycle; this is purely the dispatch-by-variant drawing.
pub(super) fn draw_hud_queue<'a>(
    renderer: &mut Renderer,
    draws: impl Iterator<Item = &'a HudDraw>,
) {
    for hud in draws {
        match hud {
            HudDraw::Text { x, y, text, fg, bg } => {
                if *x < renderer.width && *y < renderer.height {
                    renderer.draw_str(*x, *y, text, *fg, *bg);
                }
            }
            HudDraw::Box { x, y, w, h, fg, bg } => {
                renderer.draw_rect_outline(*x, *y, *w, *h, *fg, *bg)
            }
            HudDraw::Fill { x, y, w, h, ch, fg, bg } => {
                renderer.draw_rect_filled(*x, *y, *w, *h, *ch, *fg, *bg)
            }
            HudDraw::Menu { x, y, w, options, selected, fg, bg, sel_fg, sel_bg } => {
                crate::ui::Menu::new(*x, *y, *w, options.clone(), *selected)
                    .with_colors(*fg, *bg, *sel_fg, *sel_bg)
                    .draw(renderer)
            }
            HudDraw::Panel { x, y, w, h, title, fg, bg } => crate::ui::Panel::new(*x, *y, *w, *h)
                .with_title(title)
                .with_colors(*fg, *bg)
                .draw(renderer),
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

// ── Tests: Step 8-1 (docs/ember2d-master-plan.md §5.7) ──────────────────────
// Kept here rather than in play/tests.rs, which is near CLAUDE.md's 750-line
// limit — these only exercise `DrawList`, which lives in this file.
#[cfg(test)]
mod tilemap_draw_tests {
    use super::*;
    use ember2d_sim::components::{Sprite, TileDef, TilemapBuilder, Transform};
    use ember2d_sim::layers::LayerRegistry;

    /// A 100×100 wall-and-floor tilemap (floors on layer 0, one wall row on
    /// layer 1) on entity 1, plus one ordinary sprite entity on top.
    fn world_with_big_tilemap() -> World {
        let wall = TileDef {
            sprite: None,
            src: None,
            glyph: '#',
            fg: Color::Grey,
            bg: Color::Reset,
            solid: true,
            tag: String::new(),
            collider_layer: String::new(),
            texture: None,
        };
        let floor = TileDef { glyph: '.', solid: false, ..wall.clone() };
        let mut b = TilemapBuilder::new((0, 0), 100, 100).unwrap();
        for y in 0..100 {
            for x in 0..100 {
                b.add(0, x, y, floor.clone());
            }
            b.add(1, y, 0, wall.clone());
        }
        let mut map = b.finish().unwrap();
        map.refresh(&LayerRegistry::new(&["solid".to_string()]));
        let mut world = World::new();
        let id = world.spawn();
        world.add_tilemap(id, map);
        let e = world.spawn();
        world.add_transform(e, Transform::new(5.0, 5.0));
        world.add_sprite(e, Sprite::new('@', Color::Green, Color::Reset, 15));
        world
    }

    #[test]
    fn only_tilemap_cells_inside_the_view_become_draw_commands() {
        let world = world_with_big_tilemap();
        let all = DrawList::from_world(&world);
        assert_eq!(
            all.commands.len(),
            100 * 100 + 100 + 1,
            "no window: every cell plus the entity"
        );

        // A 10×5 view; `visible_cells` adds one cell of slack each side.
        let view = Rect::new(20.0, 20.0, 10.0, 5.0);
        let windowed = DrawList::from_world_in(&world, Some(view));
        let cells: Vec<&DrawCommand> = windowed.commands.iter().filter(|c| c.id == 1).collect();
        assert!(cells.len() <= 12 * 7, "a 10×5 view must not draw {} cells", cells.len());
        assert!(cells.iter().all(|c| {
            (19.0..=31.0).contains(&c.world_pos.x) && (19.0..=26.0).contains(&c.world_pos.y)
        }));
        assert!(
            windowed.commands.iter().any(|c| c.id == 2),
            "entities are never windowed out here"
        );
    }

    #[test]
    fn tilemap_cells_sort_by_their_layer_z_around_entities() {
        let world = world_with_big_tilemap();
        let list = DrawList::from_world_in(&world, Some(Rect::new(0.0, 0.0, 8.0, 8.0)));
        let zs: Vec<i32> = list.commands.iter().map(|c| c.z).collect();
        let mut sorted = zs.clone();
        sorted.sort();
        assert_eq!(zs, sorted, "draw commands must come out in z order");
        let player_at = list.commands.iter().position(|c| c.id == 2).unwrap();
        assert!(
            list.commands[..player_at].iter().all(|c| c.z <= 15),
            "floors (z 0) and walls (z 10) draw under the player (z 15)"
        );
    }
}

// ── Tests: Step 8-3 (docs/ember2d-master-plan.md §5.7) ──────────────────────
#[cfg(test)]
mod clip_frame_tests {
    use super::*;

    #[test]
    fn clip_frame_picks_the_wrapped_frame_for_glyph_and_sheet_clips() {
        let glyphs = AnimationClip {
            frames: ClipFrames::Glyphs { frames: vec!['a', 'b'] },
            fps: 4.0,
            looping: true,
        };
        assert!(matches!(clip_frame(&glyphs, 3), Some(ClipFrame::Glyph('b'))));
        let r0 = Rect::new(0.0, 0.0, 16.0, 16.0);
        let r1 = Rect::new(16.0, 0.0, 16.0, 16.0);
        let sheet = AnimationClip {
            frames: ClipFrames::Rects { texture: "s.png".to_string(), frames: vec![r0, r1] },
            fps: 4.0,
            looping: true,
        };
        match clip_frame(&sheet, 2) {
            Some(ClipFrame::Rect(path, rect)) => assert_eq!((path, rect), ("s.png", r0)),
            _ => panic!("a sheet clip draws a texture sub-rect"),
        }
        let empty = AnimationClip {
            frames: ClipFrames::Glyphs { frames: vec![] },
            fps: 1.0,
            looping: true,
        };
        assert!(clip_frame(&empty, 0).is_none());
    }
}

// ── Tests: Step 9-7 (docs/ember2d-master-plan.md §5.8) ──────────────────────
#[cfg(test)]
mod sprite_tests {
    use super::*;
    use ember2d_sim::components::{Sprite, Transform};

    #[test]
    fn a_flip_mirrors_the_source_rect_and_no_flip_leaves_it_alone() {
        let r = Rect::new(16.0, 0.0, 16.0, 16.0);
        assert_eq!(flipped_src(Some(r), 64, 32, (false, false)), Some(r));
        assert_eq!(flipped_src(None, 64, 32, (false, false)), None);
        assert_eq!(
            flipped_src(Some(r), 64, 32, (true, false)),
            Some(Rect::new(32.0, 0.0, -16.0, 16.0))
        );
        assert_eq!(
            flipped_src(None, 64, 32, (false, true)),
            Some(Rect::new(0.0, 32.0, 64.0, -32.0)),
            "a whole image flips too"
        );
    }

    #[test]
    fn y_sort_draws_the_lower_sprite_last_within_a_layer() {
        let mut world = World::new();
        let low = world.spawn();
        world.add_transform(low, Transform::new(0.0, 5.0));
        world.add_sprite(low, Sprite::glyph('a', Color::White, Color::Reset, 10));
        let high = world.spawn();
        world.add_transform(high, Transform::new(0.0, 2.0));
        world.add_sprite(high, Sprite::glyph('b', Color::White, Color::Reset, 10));
        let ids =
            |w: &World| DrawList::from_world(w).commands.iter().map(|c| c.id).collect::<Vec<_>>();
        assert_eq!(ids(&world), vec![low, high], "off: entity order");
        world.y_sort = true;
        assert_eq!(ids(&world), vec![high, low], "on: the one lower on screen is in front");
    }
}
