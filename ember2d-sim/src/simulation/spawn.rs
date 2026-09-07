// simulation/spawn.rs — Simulation::do_on_start: spawns every tile plus the
// player, compiles every script, and runs every on_start.
//
// Split into its own file rather than left in simulation.rs — Phase 6 Step 7
// (docs/ember2d-phase6-plan.md) pushed simulation.rs past the project's
// 600-line hard limit (CLAUDE.md), and `do_on_start` (moved near-verbatim
// from `ember2d::play::spawn::do_on_start` back when this type was created)
// is the single largest, most self-contained method in the file — spawning
// is its own concern, cleanly separable from the step/turn/collision
// machinery the rest of `simulation.rs` owns. Same second-`impl
// Simulation`-in-a-sibling-file pattern `apply.rs`/`api_spatial.rs` already
// established for `ScriptEngine`/`ScriptCtx` in Phase 6 Steps 2 and 7 —
// except here the sibling is a genuine CHILD module (`simulation::spawn`,
// declared via `mod spawn;` in simulation.rs, resolving to this file
// because `simulation.rs` and `simulation/` coexist — Rust 2018+'s
// non-`mod.rs` layout), not a module-tree sibling: that's what lets this
// file read `Simulation`'s private fields with no visibility bump at all.
// `do_on_start` itself needed exactly one bump the other direction —
// `pub(super)`, so `simulation.rs`'s own `on_start` (the parent module) can
// still call it. Nothing here changed behavior, only location.

use std::collections::BTreeMap;

use crate::components::{Actor, Collider, Script, Sprite, Tag, Transform};
use crate::math::Vec2;
use crate::scripting::LogEntry;
use crate::world::World;

use super::{resolve_exit_path, Simulation, StepOutcome};

impl Simulation {
    /// Spawns every tile plus the player, compiles every script, and runs
    /// `on_start` for all of them. Moved near-verbatim from
    /// `ember2d::play::spawn::do_on_start`.
    pub(super) fn do_on_start(&mut self, world: &mut World, viewport_w: usize, viewport_h: usize, persistent: &mut BTreeMap<String, rhai::Dynamic>, logs: &mut Vec<LogEntry>) {
        let mut scripts_ok = 0u32;
        let mut scripts_fail = 0u32;

        // R7 (7A-3, docs/ember2d-master-plan.md): populates `exit_targets`
        // independently of this loop's own `world.spawn()` calls — see
        // `index_exits`'s own doc comment (simulation.rs) for why, and why
        // that's what lets the same function also run on a loaded save.
        self.index_exits();

        for tile in &self.level.tiles {
            let id = world.spawn();
            world.add_transform(id, Transform::new(tile.x as f32, tile.y as f32));

            let z = tile.layer as i32 * 10;
            let mut sprite = Sprite::new(tile.glyph, tile.fg, tile.bg, z);
            if let Some(ref path) = tile.texture {
                let full = resolve_exit_path(path, &self.level.path);
                sprite = sprite.with_texture(full);
            }
            world.add_sprite(id, sprite);

            if tile.solid {
                let mut col = Collider::unit();
                let layer = if tile.collider_layer.is_empty() { "solid".to_string() } else { tile.collider_layer.clone() };
                col.set_layer(&self.layers, layer);
                col.set_mask(&self.layers, tile.collider_mask.clone());
                world.add_collider(id, col);
            } else if tile.trigger {
                let mut col = Collider::trigger(1.0, 1.0);
                col.set_layer(&self.layers, tile.collider_layer.clone());
                col.set_mask(&self.layers, tile.collider_mask.clone());
                world.add_collider(id, col);
            }

            if !tile.tag.is_empty() { world.add_tag(id, Tag::new(&tile.tag)); }
            if let Some(ref ar) = tile.actor { world.add_actor(id, Actor::ai(ar.speed)); }

            let mut source = String::new();
            if let Some(ref graph) = tile.graph { source = crate::graph::generate_graph(graph); }
            if !source.is_empty() {
                if let Some(ref path) = tile.script {
                    let full = resolve_exit_path(path, &self.level.path);
                    if let Ok(file_src) = std::fs::read_to_string(&full) { source.push('\n'); source.push_str(&file_src); }
                }
                let key = format!("__script_{}", id);
                if self.script_engine.compile_str(&key, &source, logs) { scripts_ok += 1; }
                else { scripts_fail += 1; }
                world.add_script(id, Script::new(&key));
            } else if let Some(script_path) = &tile.script {
                let full = resolve_exit_path(script_path, &self.level.path);
                world.add_script(id, Script::new(&full));
                if self.script_engine.compile(&full, logs) { scripts_ok += 1; }
                else { scripts_fail += 1; }
            }

            if tile.camera_follow && self.camera_entity.is_none() { self.camera_entity = Some(id); }
        }

        let (sx, sy) = self.level.spawn_point;
        let player = world.spawn();
        world.add_transform(player, Transform::new(sx, sy));

        let pr = &self.level.player;
        let mut p_sprite = Sprite::new(pr.glyph, pr.fg, pr.bg, pr.layer);
        if let Some(ref path) = pr.texture {
            let full = resolve_exit_path(path, &self.level.path);
            p_sprite = p_sprite.with_texture(full);
        }
        world.add_sprite(player, p_sprite);
        let mut p_col = Collider::new(pr.collider_w, pr.collider_h);
        p_col.set_layer(&self.layers, pr.collider_layer.clone());
        p_col.set_mask(&self.layers, pr.collider_mask.clone());
        world.add_collider(player, p_col);
        world.add_tag(player, Tag::new(&pr.tag));
        world.add_actor(player, Actor::local(0));

        if let Some(ref script_path) = pr.script.clone() {
            let full = resolve_exit_path(script_path, &self.level.path);
            world.add_script(player, Script::new(&full));
            if self.script_engine.compile(&full, logs) { scripts_ok += 1; }
            else { scripts_fail += 1; }
        }

        if pr.camera_follow && self.camera_entity.is_none() { self.camera_entity = Some(player); }

        if scripts_ok + scripts_fail > 0 {
            let msg = format!("{} script(s) compiled, {} failed", scripts_ok, scripts_fail);
            if scripts_fail > 0 { logs.push(LogEntry::warn(msg)); }
            else { logs.push(LogEntry::info(msg)); }
        }

        let cam_pos = self.camera_entity.map(|id| world.get_global_position(id)).unwrap_or(Vec2::ZERO);
        let game_h = (viewport_h as i32).max(1);
        let cam_x = (cam_pos.x - viewport_w as f32 / 2.0).max(0.0).round();
        let cam_y = (cam_pos.y - game_h as f32 / 2.0).max(0.0).round();

        // Phase 6 Step 3 (docs/ember2d-phase6-plan.md): `mem::take` instead
        // of `.clone()` — `self.globals`/`self.clips` are about to be
        // reassigned wholesale by `apply_script_result` right below
        // regardless (`self.globals = res.globals`), so their pre-call
        // value never needs to survive alongside a copy. Safe because
        // nothing reads `self.globals`/`self.clips` in the gap between the
        // take and that reassignment — see this module's own note on
        // `apply_script_result` for the invariant this depends on.
        let globals = std::mem::take(&mut self.globals);
        let clips = std::mem::take(&mut self.clips);
        let res = self.script_engine.run_on_start_all(
            world, logs, &self.level.extra_spawns,
            globals, clips, persistent, Vec2::new(cam_x, cam_y),
            (viewport_w, viewport_h),
        );
        let mut outcome = StepOutcome::default();
        self.apply_script_result(world, res, persistent, logs, &mut outcome);
    }
}
