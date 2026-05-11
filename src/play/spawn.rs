// play/spawn.rs — World population logic for PlayState::on_start.

use crate::components::{Collider, Script, Sprite, Tag, Transform};
use crate::event::EventBus;
use crate::math::Vec2;
use crate::scripting::LogEntry;
use crate::world::World;

use super::{resolve_exit_path, PlayState, Z_PLAYER};

impl PlayState {
    pub(super) fn do_on_start(&mut self, world: &mut World, _events: &mut EventBus) {
        let mut item_count   = 0u32;
        let mut scripts_ok   = 0u32;
        let mut scripts_fail = 0u32;

        for tile in &self.level.tiles {
            if tile.tag == "spawn" { continue; }

            let id = world.spawn();
            world.add_transform(id, Transform::new(tile.x as f32, tile.y as f32));

            let z = Self::z_for_tag(&tile.tag) + (tile.layer as i32 * 10);
            let mut sprite = Sprite::new(tile.glyph, tile.fg, tile.bg, z);
            if let Some(ref path) = tile.texture {
                let full = resolve_exit_path(path, &self.level.path);
                sprite.texture = Some(full);
            }
            world.add_sprite(id, sprite);

            if tile.solid {
                let mut col = Collider::unit();
                col.layer = tile.collider_layer.clone();
                col.mask = tile.collider_mask.clone();
                world.add_collider(id, col);
            } else if tile.trigger {
                let mut col = Collider::trigger(1.0, 1.0);
                col.layer = tile.collider_layer.clone();
                col.mask = tile.collider_mask.clone();
                world.add_collider(id, col);
                if tile.tag == "item" || tile.tag == "chest" { item_count += 1; }
            }

            if !tile.tag.is_empty() { world.add_tag(id, Tag::new(&tile.tag)); }

            // Priority 1: node graph; 2: .rhai file
            let mut source = String::new();
            if let Some(ref graph) = tile.graph {
                source = crate::editor::node_graph::generate_graph(graph);
            }
            if !source.is_empty() {
                if let Some(ref path) = tile.script {
                    let full = resolve_exit_path(path, &self.level.path);
                    if let Ok(file_src) = std::fs::read_to_string(&full) {
                        source.push('\n');
                        source.push_str(&file_src);
                    }
                }
                let key = format!("__script_{}", id);
                if self.script_engine.compile_str(&key, &source, &mut self.script_log) {
                    scripts_ok += 1;
                } else {
                    scripts_fail += 1;
                }
                world.add_script(id, Script::new(&key));
            } else if let Some(script_path) = &tile.script {
                let full = resolve_exit_path(script_path, &self.level.path);
                world.add_script(id, Script::new(&full));
                if self.script_engine.compile(&full, &mut self.script_log) {
                    scripts_ok += 1;
                } else {
                    scripts_fail += 1;
                }
            }

            if tile.camera_follow && self.camera_entity.is_none() {
                self.camera_entity = Some(id);
            }

            if let Some(ref path) = tile.next_level {
                self.exit_targets.insert(id, path.clone());
            }
        }

        self.total_items = item_count;

        // Spawn the player at the level's stored spawn point.
        let (sx, sy) = self.level.spawn_point;
        let player = world.spawn();
        world.add_transform(player, Transform::new(sx, sy));

        let pr = &self.level.player;
        let mut p_sprite = Sprite::new(pr.glyph, pr.fg, pr.bg, Z_PLAYER);
        if let Some(ref path) = pr.texture {
            let full = resolve_exit_path(path, &self.level.path);
            p_sprite.texture = Some(full);
        }
        world.add_sprite(player, p_sprite);
        // Slightly smaller than 1×1 so the player has a 0.125-unit tolerance on each
        // side when squeezing through 1-cell corridors (walls are still 1×1).
        let mut p_col = Collider::new(0.75, 0.75);
        p_col.layer = pr.collider_layer.clone();
        p_col.mask = pr.collider_mask.clone();
        world.add_collider(player, p_col);
        world.add_tag(player, Tag::new(&pr.tag));

        if let Some(ref script_path) = pr.script.clone() {
            let full = resolve_exit_path(script_path, &self.level.path);
            world.add_script(player, Script::new(&full));
            if self.script_engine.compile(&full, &mut self.script_log) {
                scripts_ok += 1;
            } else {
                scripts_fail += 1;
            }
        }

        if pr.camera_follow && self.camera_entity.is_none() {
            self.camera_entity = Some(player);
        }

        self.player_id = player;

        if scripts_ok + scripts_fail > 0 {
            let msg = format!("{} script(s) compiled, {} failed", scripts_ok, scripts_fail);
            if scripts_fail > 0 {
                self.script_log.push(LogEntry::warn(msg));
            } else {
                self.script_log.push(LogEntry::info(msg));
            }
        }

        let cam_pos = self.camera_entity
            .and_then(|id| world.transforms.get(&id))
            .map(|tf| tf.position)
            .unwrap_or(Vec2::ZERO);

        let game_h = (self.viewport_h as i32 - 2).max(1);
        let cam_x = (cam_pos.x - self.viewport_w as f32 / 2.0).max(0.0).round();
        let cam_y = (cam_pos.y - game_h as f32 / 2.0).max(0.0).round();

        let res = self.script_engine.run_on_start_all(
            world, &mut self.script_log, &self.level.extra_spawns,
            self.globals.clone(), self.persistent.clone(), Vec2::new(cam_x, cam_y),
            (self.viewport_w, self.viewport_h),
        );
        let mut dummy = false;
        self.apply_script_result(res, &mut dummy);
    }
}
