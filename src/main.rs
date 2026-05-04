// main.rs — Entry point for the ember2d demo game and level editor.
//
// ── TWO MODES ─────────────────────────────────────────────────────────────────
//
//   cargo run                          → opens a blank level editor (default)
//   cargo run -- --editor              → opens a blank level editor
//   cargo run -- --editor level.level  → opens the editor with a level file
//   cargo run -- level.level           → plays the level directly (no editor UI)
//
// The editor launches by default when no args are given.
//
// ── DEMO GAME OVERVIEW ────────────────────────────────────────────────────────
//
//   - Implementing the GameState trait
//   - Spawning entities with multiple components
//   - Smooth keyboard-driven player movement (delta-time scaled)
//   - Solid collision: walls stop the player (position rollback in late_update)
//   - Trigger collision: walking over items collects them
//   - Z-ordered rendering: floor → items → walls → player
//   - HUD: FPS counter, score, player position
//
// DEMO CONTROLS:
//   WASD or Arrow keys — move the player (@)
//   Escape             — quit

use ember2d::prelude::*;
use ember2d::editor::start_screen::StartScreen;
use ember2d::renderer::SCALE;

// ─────────────────────────────────────────────────────────────────────────────
//  Screen size detection
// ─────────────────────────────────────────────────────────────────────────────

/// Detect the primary monitor's character-cell dimensions.
/// On Windows we call GetSystemMetrics directly (no extra crate needed).
/// Other platforms fall back to a comfortable 80×24 default.
#[cfg(target_os = "windows")]
fn screen_cells() -> (usize, usize) {
    extern "system" { fn GetSystemMetrics(nIndex: i32) -> i32; }
    // SM_CXSCREEN = 0 (screen width in pixels), SM_CYSCREEN = 1 (height in pixels).
    let sw = unsafe { GetSystemMetrics(0) }.max(0) as usize;
    let sh = unsafe { GetSystemMetrics(1) }.max(0) as usize;
    // Each cell is 8px wide × 16px tall in the pixel buffer, then doubled by Scale::X2.
    let cell_w = 8 * SCALE;   // 16 px/cell on screen
    let cell_h = 16 * SCALE;  // 32 px/cell on screen
    let cols = (sw / cell_w).max(80);
    let rows = (sh / cell_h).max(24);
    (cols, rows)
}

#[cfg(not(target_os = "windows"))]
fn screen_cells() -> (usize, usize) { (80, 24) }

/// Player movement speed in cells per second.
/// Multiplied by delta_time each frame → frame-rate independent.
const PLAYER_SPEED: f32 = 10.0;

// Z-order draw layers — lower = drawn first = appears behind higher values.
const Z_FLOOR:  i32 = 0;
const Z_ITEM:   i32 = 1;
const Z_WALL:   i32 = 2;
const Z_PLAYER: i32 = 3;

// ─────────────────────────────────────────────────────────────────────────────
//  Game state
// ─────────────────────────────────────────────────────────────────────────────

/// Persistent state for the demo game, lives across all frames.
struct EmberDemo {
    /// Entity ID of the player — cached at spawn time for fast per-frame lookup.
    player_id:   EntityId,
    /// How many items have been collected.
    score:       u32,
    /// Total items placed at startup (for the "X/Y" display).
    total_items: u32,
    /// Rolling-average FPS (exponential moving average).
    fps:         f32,
}

impl EmberDemo {
    fn new() -> Self {
        EmberDemo { player_id: 0, score: 0, total_items: 0, fps: 0.0 }
    }

    // ── Spawn helpers ─────────────────────────────────────────────────────────

    fn spawn_floor(world: &mut World, x: f32, y: f32) {
        let id = world.spawn();
        world.add_transform(id, Transform::new(x, y));
        world.add_sprite(id, Sprite::new('.', Color::DarkGrey, Color::Reset, Z_FLOOR));
        world.add_tag(id, Tag::new("floor"));
        // No collider — you walk through floors.
    }

    fn spawn_wall(world: &mut World, x: f32, y: f32) {
        let id = world.spawn();
        world.add_transform(id, Transform::new(x, y));
        world.add_sprite(id, Sprite::new('#', Color::Grey, Color::Reset, Z_WALL));
        world.add_collider(id, Collider::unit()); // solid 1×1 hitbox
        world.add_tag(id, Tag::new("wall"));
    }

    fn spawn_item(world: &mut World, x: f32, y: f32) {
        let id = world.spawn();
        world.add_transform(id, Transform::new(x, y));
        world.add_sprite(id, Sprite::new('*', Color::Yellow, Color::Reset, Z_ITEM));
        // trigger(1,1) → fires Collision events but does NOT block movement.
        world.add_collider(id, Collider::trigger(1.0, 1.0));
        world.add_tag(id, Tag::new("item"));
    }

    /// Build a rectangular room: walls on the perimeter, floors inside.
    fn build_room(world: &mut World, x: usize, y: usize, w: usize, h: usize) {
        for row in y..(y + h) {
            for col in x..(x + w) {
                let on_edge =
                    row == y || row == y + h - 1 || col == x || col == x + w - 1;
                if on_edge {
                    Self::spawn_wall(world, col as f32, row as f32);
                } else {
                    Self::spawn_floor(world, col as f32, row as f32);
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  GameState implementation
// ─────────────────────────────────────────────────────────────────────────────

impl GameState for EmberDemo {
    // ── on_start ──────────────────────────────────────────────────────────────

    fn on_start(&mut self, world: &mut World, _events: &mut EventBus) {
        // Main room: 40 wide × 18 tall, top-left at (2, 2).
        // Rows 0 and SCREEN_H-1 are reserved for the HUD.
        Self::build_room(world, 2, 2, 40, 18);

        // Inner walls to create some navigation challenge.
        for col in 10..20 { Self::spawn_wall(world, col as f32, 10.0); }
        for row in 10..16 { Self::spawn_wall(world, 20.0, row as f32); }

        // Collectible items scattered around the room.
        let positions = [
            (8.0, 5.0), (12.0, 5.0), (16.0, 5.0),
            (8.0, 15.0), (12.0, 16.0), (30.0, 8.0),
            (25.0, 12.0), (35.0, 15.0),
        ];
        for &(x, y) in &positions {
            Self::spawn_item(world, x, y);
        }
        self.total_items = positions.len() as u32;

        // Spawn the player inside the room, away from walls.
        let player = world.spawn();
        world.add_transform(player, Transform::new(5.0, 5.0));
        world.add_sprite(player, Sprite::new('@', Color::Green, Color::Reset, Z_PLAYER));
        world.add_collider(player, Collider::unit());
        world.add_tag(player, Tag::new("player"));
        self.player_id = player;
    }

    // ── update (pre-physics) ──────────────────────────────────────────────────

    fn update(&mut self, ctx: UpdateContext) {
        let UpdateContext { world, input, quit, delta_time, .. } = ctx;

        // Rolling-average FPS: 90% old value + 10% new sample.
        if delta_time > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / delta_time) * 0.1;
        }

        // Quit on Escape.
        // `just_pressed` fires only on the first frame the key goes down —
        // holding Escape doesn't spam-quit.
        if input.just_pressed(Key::Escape) {
            *quit = true;
            return;
        }

        // Read movement input.
        // Both arrow keys and WASD are accepted — check both simultaneously.
        let mut dir = Vec2::ZERO;
        if input.is_held(Key::Up)    || input.is_held(Key::W) { dir.y -= 1.0; }
        if input.is_held(Key::Down)  || input.is_held(Key::S) { dir.y += 1.0; }
        if input.is_held(Key::Left)  || input.is_held(Key::A) { dir.x -= 1.0; }
        if input.is_held(Key::Right) || input.is_held(Key::D) { dir.x += 1.0; }

        // Normalize so diagonal movement isn't faster than cardinal movement.
        // Without this: NE = speed √2 ≈ 1.41×. With it: always exactly 1.0×.
        let velocity = dir.normalized() * PLAYER_SPEED;

        // Write velocity into the player's Transform.
        // The engine's physics step (which runs AFTER update() returns) will apply:
        //   position += velocity * delta_time
        if let Some(tf) = world.transforms.get_mut(&self.player_id) {
            tf.velocity = velocity;
        }
    }

    // ── late_update (post-physics, post-collision) ────────────────────────────

    fn late_update(&mut self, ctx: UpdateContext) {
        let UpdateContext { world, events, prev_positions, .. } = ctx;

        // Gather item IDs to despawn after the event loop.
        // We can't call world.despawn() inside the loop because we're also
        // borrowing `world.tags` to check entity tags — Rust won't allow
        // mutable + immutable borrows of the same struct simultaneously.
        let mut to_collect: Vec<EntityId> = Vec::new();

        for event in events.events() {
            let GameEvent::Collision { entity_a, entity_b } = event else { continue };

            // Find out if the player is one of the two entities.
            let other = if *entity_a == self.player_id {
                *entity_b
            } else if *entity_b == self.player_id {
                *entity_a
            } else {
                continue // Neither entity is the player — skip.
            };

            let other_solid = world.colliders.get(&other).map(|c| c.solid).unwrap_or(false);
            let other_tag   = world.tags.get(&other).map(|t| t.name.as_str());

            if other_solid {
                // SOLID COLLISION — push the player back to where they were
                // before this frame's physics step moved them into the wall.
                // rollback_position also zeroes velocity so they don't re-collide
                // immediately on the next frame.
                world.resolve_solid_collision(self.player_id, other, prev_positions);
            } else if other_tag == Some("item") {
                // TRIGGER COLLISION — schedule item for removal.
                to_collect.push(other);
            }
        }

        // Despawn collected items and update the score.
        for id in to_collect {
            // Guard: despawn only if still alive (avoids double-collecting).
            if world.sprites.contains_key(&id) {
                world.despawn(id);
                self.score += 1;
            }
        }
    }

    // ── render ────────────────────────────────────────────────────────────────

    fn render(&mut self, ctx: RenderContext) {
        let RenderContext { world, renderer, elapsed, .. } = ctx;

        // ── Background ─────────────────────────────────────────────────────────
        // Fill everything with the default dark background.
        renderer.draw_rect_filled(0, 0, renderer.width, renderer.height,
            ' ', Color::Reset, Color::Reset);

        // ── Sprites, sorted by z_order ─────────────────────────────────────────
        // Collect all drawable entities into a Vec so we can sort by z_order.
        // (HashMaps have no guaranteed iteration order — we need to sort manually.)
        let mut draw_list: Vec<(i32, usize, usize, char, Color, Color)> = world
            .transforms
            .iter()
            .filter_map(|(id, tf)| {
                world.sprites.get(id).and_then(|sp| {
                    if !sp.visible { return None; }
                    // Round float position to nearest cell.
                    let col = tf.position.x.round() as usize;
                    let row = tf.position.y.round() as usize;
                    if col < renderer.width && row < renderer.height {
                        Some((sp.z_order, col, row, sp.glyph, sp.fg, sp.bg))
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Ascending z_order: z=0 first (floor), z=3 last (player on top).
        draw_list.sort_unstable_by_key(|(z, ..)| *z);

        for (_, col, row, glyph, fg, bg) in draw_list {
            renderer.draw_char(col, row, glyph, fg, bg);
        }

        // ── Item pulse animation ───────────────────────────────────────────────
        // Alternate item color every ~0.33 s using elapsed time.
        // `(elapsed * 3.0) as u32 % 2` toggles between 0 and 1 three times/sec.
        if (elapsed * 3.0) as u32 % 2 == 0 {
            for (id, tf) in &world.transforms {
                if world.tags.get(id).map(|t| t.name == "item").unwrap_or(false) {
                    let col = tf.position.x.round() as usize;
                    let row = tf.position.y.round() as usize;
                    if col < renderer.width && row < renderer.height {
                        renderer.draw_char(col, row, '*', Color::White, Color::Reset);
                    }
                }
            }
        }

        // ── HUD — top bar ──────────────────────────────────────────────────────
        renderer.draw_rect_filled(0, 0, renderer.width, 1, ' ', Color::Black, Color::DarkBlue);
        renderer.draw_str(1, 0, "EMBER2D", Color::White, Color::DarkBlue);

        let score_str = format!("  Items {}/{}", self.score, self.total_items);
        renderer.draw_str(10, 0, &score_str, Color::Yellow, Color::DarkBlue);

        if let Some(tf) = world.transforms.get(&self.player_id) {
            let pos_str = format!("  x:{:.1} y:{:.1}", tf.position.x, tf.position.y);
            renderer.draw_str(28, 0, &pos_str, Color::Green, Color::DarkBlue);
        }

        let fps_str = format!("FPS:{:3.0}", self.fps);
        let fps_col = renderer.width.saturating_sub(fps_str.len() + 1);
        renderer.draw_str(fps_col, 0, &fps_str, Color::Cyan, Color::DarkBlue);

        // ── HUD — bottom bar ───────────────────────────────────────────────────
        renderer.draw_rect_filled(0, renderer.height - 1, renderer.width, 1,
            ' ', Color::Black, Color::DarkGrey);
        renderer.draw_str(1, renderer.height - 1,
            "WASD / Arrows: Move   *: Collect items   Esc: Quit",
            Color::White, Color::DarkGrey);

        // ── Victory ────────────────────────────────────────────────────────────
        if self.total_items > 0 && self.score >= self.total_items {
            let msg = " ALL ITEMS COLLECTED!  Press Esc to quit. ";
            let col = (renderer.width / 2).saturating_sub(msg.len() / 2);
            let row = renderer.height / 2;
            renderer.draw_rect_filled(
                col.saturating_sub(1), row - 1, msg.len() + 2, 3,
                ' ', Color::Black, Color::Green,
            );
            renderer.draw_str(col, row, msg, Color::Black, Color::Green);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Entry point
// ─────────────────────────────────────────────────────────────────────────────

fn main() {
    // ── Mode detection ────────────────────────────────────────────────────────
    //
    // CLI argument matrix:
    //
    //   (no args)                   → open level editor (blank canvas)
    //   --editor                    → open level editor (blank canvas)
    //   --editor path/to/file.level → open level editor with a saved level
    //   path/to/file.level          → play the level directly (no editor UI)
    //
    // We detect mode by scanning args:
    //   - "--editor" flag OR no args → editor mode
    //   - a *.level filename without "--editor" → direct play
    let args: Vec<String> = std::env::args().collect();
    let has_level_file = args.iter().skip(1).any(|a| a.ends_with(".level") && !a.starts_with('-'));
    let editor_mode = args.iter().any(|a| a == "--editor") || (!has_level_file && args.len() == 1);

    // Find a .level file arg that isn't the editor flag itself.
    let level_file: Option<&str> = args.iter()
        .skip(1)
        .find(|a| a.ends_with(".level") && !a.starts_with('-'))
        .map(String::as_str);

    let (screen_w, screen_h) = screen_cells();

    if editor_mode {
        // ── Level editor with Editor ↔ Play switching ────────────────────────
        //
        // run_editor_app handles F5 (go to play) and Escape (return to editor)
        // in a loop — so we don't need `engine.run()` directly here.

        let level_path = args.iter()
            .skip_while(|a| *a != "--editor")
            .nth(1)
            .map(String::as_str)
            .unwrap_or("");

        let mut engine = Engine::new(screen_w, screen_h, "Ember2D Level Editor")
            .expect("Failed to open window.");

        let editor = if level_path.is_empty() {
            // Show the start screen so the user can name/configure the project.
            let mut start = StartScreen::new();
            if let Err(e) = engine.run(&mut start) {
                eprintln!("Start screen error: {}", e);
                std::process::exit(1);
            }
            engine.reset_world();
            match start.result {
                None => return,  // user quit from the start screen
                Some(result) => EditorState::new_from_result(result).unwrap_or_else(|e| {
                    eprintln!("Warning: {}", e);
                    EditorState::new("")
                }),
            }
        } else {
            EditorState::load(level_path).unwrap_or_else(|e| {
                eprintln!("Warning: {}", e);
                EditorState::new(level_path)
            })
        };

        if let Err(e) = run_editor_app(&mut engine, editor) {
            eprintln!("Editor error: {}", e);
            std::process::exit(1);
        }

    } else if let Some(path) = level_file {
        // ── Direct play: load a .level file and run it ───────────────────────

        let data = LevelData::load(path).unwrap_or_else(|e| {
            eprintln!("Failed to load level '{}': {}", path, e);
            std::process::exit(1);
        });

        let mut engine = Engine::new(screen_w, screen_h, &format!("Ember2D — {}", path))
            .expect("Failed to open window.");

        if let Err(e) = run_play_app(&mut engine, data) {
            eprintln!("Play error: {}", e);
            std::process::exit(1);
        }

    } else {
        // ── Demo game ─────────────────────────────────────────────────────────

        let mut engine = Engine::new(screen_w, screen_h, "Ember2D Demo")
            .expect("Failed to open window.");

        let mut game = EmberDemo::new();

        if let Err(e) = engine.run(&mut game) {
            eprintln!("Engine error: {}", e);
            std::process::exit(1);
        }
    }
}
