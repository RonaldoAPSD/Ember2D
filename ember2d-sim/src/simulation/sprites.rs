// simulation/sprites.rs — the half of a script's sprite requests that needs
// a file found: `set_sprite` (a tileset region) and `play_project_clip`.
//
// Step 9-7 (docs/ember2d-master-plan.md §5.8). The script engine queues
// these (`scripting/sprite.rs`) and hands them back in
// `ScriptUpdateResult::sprite_requests`; this resolves them with the same
// `TilesetResolver` level load uses — through the simulation's own
// `LevelSource`, so this crate still never touches the filesystem itself.
// Results (and failures) are cached for the life of the level, so a script
// that calls `set_sprite` every frame reads each tileset once and warns
// about a missing region once.

use std::rc::Rc;

use crate::components::{Animator, SpriteSource};
use crate::math::Vec2;
use crate::scripting::{LogEntry, SpriteOp};
use crate::world::{EntityId, World};

use super::tilesets::TilesetResolver;
use super::Simulation;

impl Simulation {
    pub(super) fn apply_sprite_requests(
        &mut self,
        world: &mut World,
        requests: Vec<SpriteOp>,
        logs: &mut Vec<LogEntry>,
    ) {
        if requests.is_empty() {
            return;
        }
        let level_path = self.level.path.clone();
        let source = Rc::clone(&self.level_source);
        let mut resolver = TilesetResolver::new(&level_path, &*source);
        for op in requests {
            match op {
                SpriteOp::Region(id, sprite) => {
                    let key = (sprite.tileset.clone(), sprite.region.clone());
                    if !self.sprite_regions.contains_key(&key) {
                        let found = resolver.resolve(&sprite);
                        if let Err(why) = &found {
                            logs.push(LogEntry::warn(format!(
                                "set_sprite({}, {}): {why}",
                                sprite.tileset, sprite.region
                            )));
                        }
                        self.sprite_regions.insert(key.clone(), found);
                    }
                    let Some(Ok((image, rect))) = self.sprite_regions.get(&key) else { continue };
                    if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) {
                        sp.source = SpriteSource::Texture { path: image.clone(), src: Some(*rect) };
                        // One level cell, like a painted sprite tile, and
                        // drawn as the art is: a glyph's colour would
                        // otherwise tint the image (`set_tint` after this
                        // still can).
                        sp.size.get_or_insert(Vec2::new(1.0, 1.0));
                        sp.tint = crate::color::Color::White;
                    }
                }
                SpriteOp::ProjectClip(id, name, once) => {
                    if !self.clips.contains_key(&name) {
                        if self.missing_clips.contains(&name) {
                            continue;
                        }
                        match resolver.resolve_clip(&name) {
                            Ok(clip) => {
                                self.clips.insert(name.clone(), clip);
                            }
                            Err(why) => {
                                logs.push(LogEntry::warn(format!(
                                    "play_project_clip('{name}'): {why}"
                                )));
                                self.missing_clips.insert(name);
                                continue;
                            }
                        }
                    }
                    let eid = id as EntityId;
                    if !world.transforms.contains_key(&eid) {
                        continue;
                    }
                    // The same restart `play_clip` does (scripting/apply.rs).
                    let animator =
                        world.animators.entry(eid).or_insert_with(|| Animator::new(name.clone()));
                    animator.clip = name.clone();
                    animator.frame = 0;
                    animator.elapsed = 0.0;
                    animator.playing = true;
                    animator.oneshot = once;
                    if let Some(sp) = world.sprites.get_mut(&eid) {
                        sp.source = SpriteSource::Clip { name };
                        sp.size.get_or_insert(Vec2::new(1.0, 1.0));
                        sp.tint = crate::color::Color::White; // as `set_sprite`
                    }
                }
                // Size/flip/y-sort never get here (`apply_sprite_ops`).
                _ => {}
            }
        }
    }
}
