// play/spawn.rs — World population logic for PlayState::on_start.

use std::collections::HashMap;
use crate::components::{Collider, Script, Sprite, Tag, Transform};
use crate::event::EventBus;
use crate::math::Vec2;
use crate::scripting::LogEntry;
use crate::world::World;

use super::{resolve_exit_path, PlayState};

impl PlayState {
    pub(super) fn do_on_start(&mut self, world: &mut World, _events: &mut EventBus, viewport_width: usize, viewport_height: usize, persistent: &mut HashMap<String, rhai::Dynamic>) {
        let mut scripts_ok   = 0u32;
        let mut scripts_fail = 0u32;

        for tile in &self.level.tiles {
            let id = world.spawn();
            world.add_transform(id, Transform::new(tile.x as f32, tile.y as f32));

            // Phase 4: was `z_for_tag(&tile.tag) + layer*10` — the per-tag
            // sub-ordering never actually resolved a real conflict, since
            // `LevelGrid`'s tiles are keyed by `(x, y, layer)`: two tiles on
            // the same layer can't occupy the same cell in the first place,
            // so their relative z never affects what's visibly drawn on top
            // of what. The layer tiers alone (0/10/20) already keep every
            // tile between Z_PLAYER's 15, exactly preserving the intended
            // Background < Main < Player < Foreground stacking.
            let z = tile.layer as i32 * 10;
            let mut sprite = Sprite::new(tile.glyph, tile.fg, tile.bg, z);
            if let Some(ref path) = tile.texture {
                let full = resolve_exit_path(path, &self.level.path);
                sprite = sprite.with_texture(full);
            }
            world.add_sprite(id, sprite);

            if tile.solid {
                let mut col = Collider::unit();
                // Solid tiles default to the "solid" layer so a mask like
                // ["solid"] hits any generic wall the author didn't bother
                // naming. Triggers get no such default — see the branch below.
                col.layer = if tile.collider_layer.is_empty() { "solid".to_string() } else { tile.collider_layer.clone() };
                col.mask = tile.collider_mask.clone();
                world.add_collider(id, col);
            } else if tile.trigger {
                let mut col = Collider::trigger(1.0, 1.0);
                // Defect D4: this used to default to "solid" too, which meant
                // an unlabeled trigger (item, chest, danger zone) matched any
                // mask like ["solid"] meant only for real obstacles, corrupting
                // collision filtering. A trigger with no explicit layer just
                // stays unlabeled — same empty default Collider::trigger() uses.
                col.layer = tile.collider_layer.clone();
                col.mask = tile.collider_mask.clone();
                world.add_collider(id, col);
            }

            if !tile.tag.is_empty() { world.add_tag(id, Tag::new(&tile.tag)); }

            // Step 3d moved the normal case (a saved level) to save time —
            // `EditorState::save`'s sidecar migration already dropped `graph`
            // and repointed `script` at the generated `.rhai` by the time a
            // saved file reaches here, so this branch is a no-op for it. It
            // stays live for the one case that never goes through `save`: a
            // still-unsaved level played via F5/Play preview, where a tile's
            // graph exists only in the editor's in-memory grid.
            let mut source = String::new();
            if let Some(ref graph) = tile.graph { source = crate::editor::node_graph::generate_graph(graph); }
            if !source.is_empty() {
                if let Some(ref path) = tile.script {
                    let full = resolve_exit_path(path, &self.level.path);
                    if let Ok(file_src) = std::fs::read_to_string(&full) { source.push('\n'); source.push_str(&file_src); }
                }
                let key = format!("__script_{}", id);
                if self.script_engine.compile_str(&key, &source, &mut self.script_log) { scripts_ok += 1; } 
                else { scripts_fail += 1; }
                world.add_script(id, Script::new(&key));
            } else if let Some(script_path) = &tile.script {
                let full = resolve_exit_path(script_path, &self.level.path);
                world.add_script(id, Script::new(&full));
                if self.script_engine.compile(&full, &mut self.script_log) { scripts_ok += 1; } 
                else { scripts_fail += 1; }
            }

            if tile.camera_follow && self.camera_entity.is_none() { self.camera_entity = Some(id); }
            if let Some(ref path) = tile.next_level { self.exit_targets.insert(id, path.clone()); }
        }

        let (sx, sy) = self.level.spawn_point;
        let player = world.spawn();
        world.add_transform(player, Transform::new(sx, sy));

        let pr = &self.level.player;
        // Step 4g: was a hardcoded Z_PLAYER constant — now a PlayerRecord
        // field, editable per project like collider_w/collider_h already are.
        let mut p_sprite = Sprite::new(pr.glyph, pr.fg, pr.bg, pr.layer);
        if let Some(ref path) = pr.texture {
            let full = resolve_exit_path(path, &self.level.path);
            p_sprite = p_sprite.with_texture(full);
        }
        world.add_sprite(player, p_sprite);
        // Phase 4: was a hardcoded Collider::new(0.75, 0.75) — now a
        // PlayerRecord field, editable per project instead of fixed engine-wide.
        let mut p_col = Collider::new(pr.collider_w, pr.collider_h);
        p_col.layer = pr.collider_layer.clone(); p_col.mask = pr.collider_mask.clone();
        world.add_collider(player, p_col);
        world.add_tag(player, Tag::new(&pr.tag));

        if let Some(ref script_path) = pr.script.clone() {
            let full = resolve_exit_path(script_path, &self.level.path);
            world.add_script(player, Script::new(&full));
            if self.script_engine.compile(&full, &mut self.script_log) { scripts_ok += 1; } 
            else { scripts_fail += 1; }
        }

        if pr.camera_follow && self.camera_entity.is_none() { self.camera_entity = Some(player); }
        self.player_id = player;

        if scripts_ok + scripts_fail > 0 {
            let msg = format!("{} script(s) compiled, {} failed", scripts_ok, scripts_fail);
            if scripts_fail > 0 { self.script_log.push(LogEntry::warn(msg)); } 
            else { self.script_log.push(LogEntry::info(msg)); }
        }

        let cam_pos = self.camera_entity.map(|id| world.get_global_position(id)).unwrap_or(Vec2::ZERO);
        // Step 4g: full viewport height — no HUD bars reserving rows anymore.
        let game_h = (viewport_height as i32).max(1);
        let cam_x = (cam_pos.x - viewport_width as f32 / 2.0).max(0.0).round();
        let cam_y = (cam_pos.y - game_h as f32 / 2.0).max(0.0).round();

        let res = self.script_engine.run_on_start_all(
            world, &mut self.script_log, &self.level.extra_spawns,
            self.globals.clone(), self.clips.clone(), persistent, Vec2::new(cam_x, cam_y),
            (viewport_width, viewport_height),
        );
        let mut dummy = false;
        self.apply_script_result(world, res, &mut dummy, persistent);
    }
}
