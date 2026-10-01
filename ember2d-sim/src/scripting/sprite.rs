// scripting/sprite.rs — scripts changing how an entity is drawn:
// `set_size`, `set_flip`, `set_sprite` (a named tileset region),
// `play_project_clip` (a clip from the project's `assets/clips/`) and
// `set_y_sort`.
//
// Step 9-7 (docs/ember2d-master-plan.md §5.8), for the sprite-based RPG
// demo. Before it a script could only pick a whole image (`set_texture`) or
// a clip some tile in the level already used; a character facing left, a
// door swapping to its open region, or a monster sprite chosen at spawn
// all needed one of these.
//
// Everything queues a `SpriteOp`, like every other deferred write. Size,
// flip and y-sort are plain `World` writes, applied in `apply_ctx`. A
// region or a project clip has to be FOUND first (a tileset or clip file,
// read through the simulation's `LevelSource`), which the script engine
// can't do, so those two travel back in `ScriptUpdateResult::sprite_requests`
// and `Simulation` resolves them (simulation/sprites.rs) with the same
// loader level load uses.

use crate::math::Vec2;
use crate::tileset::SpriteRef;
use crate::world::{EntityId, World};

use super::api::ScriptCtx;

/// One sprite request from a script.
#[derive(Debug, Clone, PartialEq)]
pub enum SpriteOp {
    /// World-space size; `None` = natural size.
    Size(i64, Option<Vec2>),
    Flip(i64, bool, bool),
    /// Draw the entity as a named region of a project tileset.
    Region(i64, SpriteRef),
    /// Play the project clip `assets/clips/<name>.ron`; `true` = once.
    ProjectClip(i64, String, bool),
    YSort(bool),
}

/// Applies the ops that need nothing but `World`; returns the rest (the
/// ones that need a file found) in call order.
pub(super) fn apply_sprite_ops(world: &mut World, ops: Vec<SpriteOp>) -> Vec<SpriteOp> {
    let mut deferred = Vec::new();
    for op in ops {
        match op {
            SpriteOp::Size(id, size) => {
                if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) {
                    sp.size = size;
                }
            }
            SpriteOp::Flip(id, fx, fy) => {
                if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) {
                    sp.flip_x = fx;
                    sp.flip_y = fy;
                }
            }
            SpriteOp::YSort(on) => world.y_sort = on,
            other => deferred.push(other),
        }
    }
    deferred
}

impl ScriptCtx {
    /// Draw `id` at `w`×`h` world units (one world unit is one level cell).
    /// A width or height of 0 or less goes back to natural size.
    pub fn set_size(&mut self, id: i64, w: f64, h: f64) {
        let ok = w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0;
        let size = ok.then(|| Vec2::new(w as f32, h as f32));
        self.inner.borrow_mut().sprite_ops.push(SpriteOp::Size(id, size));
    }
    // Whole numbers and mixed ones too (`set_size(id, 2, 1.5)`) — Rhai
    // matches an overload by exact argument types.
    fn set_size_int(&mut self, id: i64, w: i64, h: i64) {
        self.set_size(id, w as f64, h as f64);
    }
    fn set_size_int_float(&mut self, id: i64, w: i64, h: f64) {
        self.set_size(id, w as f64, h);
    }
    fn set_size_float_int(&mut self, id: i64, w: f64, h: i64) {
        self.set_size(id, w, h as f64);
    }

    /// Mirror `id`'s image left-right (`fx`) and/or upside down (`fy`).
    /// Images and clip frames only — a glyph draws as it is.
    pub fn set_flip(&mut self, id: i64, fx: bool, fy: bool) {
        self.inner.borrow_mut().sprite_ops.push(SpriteOp::Flip(id, fx, fy));
    }

    /// Draw `id` as region `region` of the project tileset `tileset`, one
    /// level cell big (like a painted sprite tile) — `set_size` after it
    /// changes that. A tileset or region that doesn't exist logs a warning
    /// and leaves the sprite alone.
    pub fn set_sprite(&mut self, id: i64, tileset: String, region: String) {
        let op = SpriteOp::Region(id, SpriteRef::new(tileset, region));
        self.inner.borrow_mut().sprite_ops.push(op);
    }

    /// Loop the project clip `name` (`assets/clips/<name>.ron`, built in
    /// File > Animation Clips) on `id`, loading it the first time — unlike
    /// `play_clip`, which only knows clips registered by a script or used
    /// by a tile.
    pub fn play_project_clip(&mut self, id: i64, name: String) {
        self.inner.borrow_mut().sprite_ops.push(SpriteOp::ProjectClip(id, name, false));
    }

    /// `play_project_clip`, once, stopping on the last frame.
    pub fn play_project_clip_once(&mut self, id: i64, name: String) {
        self.inner.borrow_mut().sprite_ops.push(SpriteOp::ProjectClip(id, name, true));
    }

    /// Turn y-sorting on or off for this level: among sprites with the same
    /// layer order, whichever sits lower on screen draws in front — what a
    /// top-down game wants for characters walking past each other.
    pub fn set_y_sort(&mut self, on: bool) {
        self.inner.borrow_mut().sprite_ops.push(SpriteOp::YSort(on));
    }
}

pub(super) fn register(engine: &mut rhai::Engine) {
    engine.register_fn("set_size", ScriptCtx::set_size);
    engine.register_fn("set_size", ScriptCtx::set_size_int);
    engine.register_fn("set_size", ScriptCtx::set_size_int_float);
    engine.register_fn("set_size", ScriptCtx::set_size_float_int);
    engine.register_fn("set_flip", ScriptCtx::set_flip);
    engine.register_fn("set_sprite", ScriptCtx::set_sprite);
    engine.register_fn("play_project_clip", ScriptCtx::play_project_clip);
    engine.register_fn("play_project_clip_once", ScriptCtx::play_project_clip_once);
    engine.register_fn("set_y_sort", ScriptCtx::set_y_sort);
}
