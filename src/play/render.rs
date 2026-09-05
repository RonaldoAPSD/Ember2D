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
// `Sprite` has no field distinct from `z_order` to sort by, so there's
// nothing to add without inventing data that doesn't exist. `z_order`
// already folds in the tile's authored layer (`z_for_tag(tag) + layer*10`,
// see play/spawn.rs), so this isn't a functional gap, just a naming one
// Phase 3's sprite model may resolve.
//
// Step 2e: commands now carry a real world-space position instead of a
// pre-subtracted screen col/row — `Space::World` was a lie otherwise
// (screen coordinates labeled "World"). The camera conversion happens once,
// in `render`, via `Camera::world_to_screen`.

use crate::components::SpriteSource;
use crate::math::Vec2;
use crate::renderer::color::Color;
use crate::world::{EntityId, World};

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
