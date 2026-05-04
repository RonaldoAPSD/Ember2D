// engine.rs — The main game engine: the game loop and the trait your game implements.
//
// WHAT CHANGED FROM THE CROSSTERM VERSION:
//   - No more terminal raw mode, alternate screen, or cursor hiding.
//     The Renderer now owns a real OS window — no terminal setup needed.
//   - Input is polled from the Renderer's window (minifb processes events
//     during update_with_buffer), not from crossterm's event stream.
//   - We check renderer.is_open() to detect the user closing the window.
//   - Engine::new() now takes a `title` parameter for the window title.
//
// Everything else — the game loop order, GameState trait, context structs —
// is identical to before. Switching backends didn't change the game API.
//
// ──────────────────────────────────────────────────────────────────────────────
//  GAME LOOP ORDER (same as before)
// ──────────────────────────────────────────────────────────────────────────────
//
//  loop {
//      1. Poll input          (read keys from last frame's window update)
//      2. Check quit          (window closed, or game set ctx.quit = true)
//      3. Clear events
//      4. Snapshot positions  (save positions BEFORE physics moves entities)
//      5. update()            (game sets velocities, reads input)
//      6. Physics             (velocity × delta_time → position)
//      7. Collision detect    (find overlapping pairs → Collision events)
//      8. late_update()       (game rolls back solid collisions, collects items)
//      9. Render              (game draws to back buffer → present flips it)
//     10. Frame cap           (sleep to target 60 FPS)
//  }

use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

use minifb;

use crate::event::EventBus;
use crate::input::InputManager;
use crate::level::LevelData;
use crate::math::Vec2;
use crate::mouse::MouseState;
use crate::renderer::Renderer;
use crate::world::{EntityId, World};

// ── Mode transition ───────────────────────────────────────────────────────────

/// Signals a requested mode switch, returned by `Engine::run_until_transition`.
///
/// A GameState implementation sets this via its own internal flag, then the
/// engine detects it at the end of the frame and returns it to the caller
/// (usually `run_app` in `src/app.rs`).
///
/// Example flow:
///   EditorState receives F5 → stores `Some(Transition::ToPlay(data))`
///   Engine detects it → returns `Ok(Some(Transition::ToPlay(data)))` to run_app
///   run_app creates PlayState and calls run_until_transition again
pub enum Transition {
    /// Switch to play mode, loading the given level data.
    ToPlay(LevelData),
    /// Switch back to the level editor.
    ToEditor,
    /// Switch back to the start screen.
    ToStart,
}

// ── Frame rate ────────────────────────────────────────────────────────────────

/// Target frames per second.
const TARGET_FPS: u64 = 60;

/// How long each frame should take to hit TARGET_FPS.
/// We sleep the remainder at the end of each frame.
const FRAME_DURATION: Duration = Duration::from_micros(1_000_000 / TARGET_FPS);

// ── Context structs ───────────────────────────────────────────────────────────

/// Everything the game needs during `update()` and `late_update()`.
///
/// Destructure it for convenience:
///   `let UpdateContext { world, input, mouse, events, .. } = ctx;`
pub struct UpdateContext<'a> {
    /// Full read/write access to entities and components.
    pub world: &'a mut World,

    /// Current keyboard state — use `input.is_held(Key::W)`, etc.
    pub input: &'a InputManager,

    /// Current mouse position and button state.
    /// Use `mouse.left_just_pressed()`, `mouse.cell_x`, etc.
    pub mouse: &'a MouseState,

    /// The event bus: read collision events, emit custom events.
    pub events: &'a mut EventBus,

    /// Positions of all entities at the START of this frame (before physics).
    /// In `late_update()`, pass this to `world.rollback_position(id, prev_positions)`
    /// to move a colliding entity back to where it was before it hit something.
    pub prev_positions: &'a HashMap<EntityId, Vec2>,

    /// Seconds since the last frame. Multiply velocities by this for frame-rate independence.
    pub delta_time: f32,

    /// Seconds since the engine started. Use for animations, pulsing effects, timers.
    pub elapsed: f32,

    /// Set to `true` from game code to exit the game loop gracefully.
    pub quit: &'a mut bool,
}

/// Everything the game needs during `render()`.
pub struct RenderContext<'a> {
    /// Read-only entity/component data — draw it but don't change it here.
    pub world: &'a World,

    /// Call `renderer.draw_char(...)`, `draw_str(...)`, etc. to fill the frame.
    pub renderer: &'a mut Renderer,

    /// Current mouse position and button state (read-only during render).
    pub mouse: &'a MouseState,

    /// Seconds since the last frame. Useful for animated sprites.
    pub delta_time: f32,

    /// Seconds since the engine started. Useful for pulsing and scrolling effects.
    pub elapsed: f32,
}

// ── GameState trait ───────────────────────────────────────────────────────────

/// Implement this trait to write a game using ember2d.
///
/// # Minimal example
/// ```rust
/// struct MyGame;
/// impl GameState for MyGame {
///     fn on_start(&mut self, world: &mut World, _: &mut EventBus) {
///         let id = world.spawn();
///         world.add_transform(id, Transform::new(5.0, 5.0));
///         world.add_sprite(id, Sprite::simple('@', Color::Green));
///     }
///     fn update(&mut self, _ctx: UpdateContext) {}
///     fn render(&mut self, ctx: RenderContext) {
///         // draw_* calls here
///     }
/// }
/// fn main() {
///     Engine::new(80, 24, "My Game").unwrap().run(&mut MyGame).unwrap();
/// }
/// ```
pub trait GameState {
    /// Called once before the game loop. Spawn entities, build maps, set initial state.
    fn on_start(&mut self, world: &mut World, events: &mut EventBus);

    /// Called every frame BEFORE physics.
    /// Set velocities from input here. Physics hasn't moved anything yet.
    fn update(&mut self, ctx: UpdateContext);

    /// Called every frame AFTER physics and collision detection.
    /// Collision events are in `ctx.events`. Roll back solid collisions here.
    /// Has a default empty implementation — omit it if you don't need it.
    fn late_update(&mut self, _ctx: UpdateContext) {}

    /// Called every frame after late_update. Draw entities to the renderer here.
    fn render(&mut self, ctx: RenderContext);

    /// Optional: return a pending mode transition.
    ///
    /// The engine calls this once per frame, after render. If it returns Some,
    /// the engine stops the current game loop and returns the transition to the
    /// caller. Implement this if your GameState needs to switch modes (e.g.
    /// F5 in the editor goes to play mode, Escape in play returns to editor).
    ///
    /// The default returns None — most GameStates don't need mode switching.
    fn take_transition(&mut self) -> Option<Transition> { None }
}

// ── Engine ────────────────────────────────────────────────────────────────────

/// The main engine: owns all core systems and runs the game loop.
pub struct Engine {
    renderer: Renderer,
    world:    World,
    input:    InputManager,
    mouse:    MouseState,
    events:   EventBus,
    /// Viewport width in character columns.
    pub width: usize,
    /// Viewport height in character rows.
    pub height: usize,
}

impl Engine {
    /// Create a new engine with a `width × height` character viewport.
    ///
    /// `title` is the OS window title bar text.
    ///
    /// This opens the window immediately. If the display server is unavailable
    /// (e.g. running headless on a server) this returns an error.
    pub fn new(width: usize, height: usize, title: &str) -> io::Result<Self> {
        let renderer = Renderer::new(width, height, title)?;
        Ok(Engine {
            renderer,
            world:  World::new(),
            input:  InputManager::new(),
            mouse:  MouseState::new(),
            events: EventBus::new(),
            width,
            height,
        })
    }

    /// Reset the ECS world and clear the event queue.
    ///
    /// Call this before handing control to a new GameState so it starts with
    /// a clean slate — no leftover entities from the previous mode.
    pub fn reset_world(&mut self) {
        self.world  = World::new();
        self.events = EventBus::new();
    }

    /// Run until the game exits OR signals a mode transition.
    ///
    /// Identical to `run()` except that after each frame it calls
    /// `game.take_transition()`. If that returns `Some(t)`, the loop stops
    /// and `Ok(Some(t))` is returned so the caller can switch modes.
    /// Returns `Ok(None)` if the game exits normally (window closed / quit flag).
    pub fn run_until_transition<G: GameState>(&mut self, game: &mut G) -> io::Result<Option<Transition>> {
        game.on_start(&mut self.world, &mut self.events);

        let start_time = Instant::now();
        let mut last_frame = Instant::now();
        let mut should_quit = false;

        loop {
            let now        = Instant::now();
            let delta_time = now.duration_since(last_frame).as_secs_f32();
            let elapsed    = now.duration_since(start_time).as_secs_f32();
            last_frame = now;

            let current_keys = self.renderer.current_keys();
            let window_open  = self.renderer.is_open();
            self.input.poll(current_keys, window_open);

            let mouse_pos   = self.renderer.current_mouse_pos();
            let left_down   = self.renderer.is_mouse_button_down(minifb::MouseButton::Left);
            let right_down  = self.renderer.is_mouse_button_down(minifb::MouseButton::Right);
            let middle_down = self.renderer.is_mouse_button_down(minifb::MouseButton::Middle);
            let scroll      = self.renderer.get_scroll_wheel();
            self.mouse.poll(mouse_pos, left_down, right_down, middle_down, scroll);

            if self.input.quit_requested { break; }

            self.events.clear();
            let prev_positions = self.world.snapshot_positions();

            game.update(UpdateContext {
                world:          &mut self.world,
                input:          &self.input,
                mouse:          &self.mouse,
                events:         &mut self.events,
                prev_positions: &prev_positions,
                delta_time,
                elapsed,
                quit:           &mut should_quit,
            });

            if should_quit { break; }

            self.world.integrate_physics(delta_time);
            self.world.detect_collisions(&mut self.events);

            game.late_update(UpdateContext {
                world:          &mut self.world,
                input:          &self.input,
                mouse:          &self.mouse,
                events:         &mut self.events,
                prev_positions: &prev_positions,
                delta_time,
                elapsed,
                quit:           &mut should_quit,
            });

            if should_quit { break; }

            self.renderer.clear();
            game.render(RenderContext {
                world:    &self.world,
                renderer: &mut self.renderer,
                mouse:    &self.mouse,
                delta_time,
                elapsed,
            });
            self.renderer.present()?;

            // Sync engine dimensions if the window was resized.
            if self.renderer.try_handle_resize() {
                self.width  = self.renderer.width;
                self.height = self.renderer.height;
            }

            // Check for mode transition AFTER rendering the last frame.
            if let Some(t) = game.take_transition() {
                return Ok(Some(t));
            }

            let frame_elapsed = Instant::now().duration_since(now);
            if frame_elapsed < FRAME_DURATION {
                std::thread::sleep(FRAME_DURATION - frame_elapsed);
            }
        }

        Ok(None)
    }

    /// Start the game loop and hand control to `game`.
    ///
    /// Blocks until the game exits (window closed, or `ctx.quit = true`).
    pub fn run<G: GameState>(&mut self, game: &mut G) -> io::Result<()> {
        // ── Initialize ─────────────────────────────────────────────────────
        game.on_start(&mut self.world, &mut self.events);

        let start_time = Instant::now();
        let mut last_frame = Instant::now();
        let mut should_quit = false;

        // ── Main game loop ─────────────────────────────────────────────────
        loop {
            // ── 1. Timing ──────────────────────────────────────────────────
            let now = Instant::now();
            let delta_time = now.duration_since(last_frame).as_secs_f32();
            let elapsed    = now.duration_since(start_time).as_secs_f32();
            last_frame = now;

            // ── 2. Input ───────────────────────────────────────────────────
            // After last frame's `update_with_buffer`, minifb has the current key state.
            // We poll it now so `update()` sees fresh input.
            let current_keys = self.renderer.current_keys();
            let window_open  = self.renderer.is_open();
            self.input.poll(current_keys, window_open);

            // Poll mouse: convert raw window-space coords to cell coords.
            let mouse_pos   = self.renderer.current_mouse_pos();
            let left_down   = self.renderer.is_mouse_button_down(minifb::MouseButton::Left);
            let right_down  = self.renderer.is_mouse_button_down(minifb::MouseButton::Right);
            let middle_down = self.renderer.is_mouse_button_down(minifb::MouseButton::Middle);
            let scroll      = self.renderer.get_scroll_wheel();
            self.mouse.poll(mouse_pos, left_down, right_down, middle_down, scroll);

            if self.input.quit_requested {
                break;
            }

            // ── 3. Events ──────────────────────────────────────────────────
            // Clear last frame's events. A fresh set is built below.
            self.events.clear();

            // ── 4. Snapshot positions ──────────────────────────────────────
            // Record where each entity is BEFORE physics moves them.
            // Passed to late_update so the game can roll back collisions.
            let prev_positions = self.world.snapshot_positions();

            // ── 5. Game update (pre-physics) ───────────────────────────────
            // Game code reads input and sets velocities.
            // Nothing has moved yet — entities are at prev_positions.
            game.update(UpdateContext {
                world:          &mut self.world,
                input:          &self.input,
                mouse:          &self.mouse,
                events:         &mut self.events,
                prev_positions: &prev_positions,
                delta_time,
                elapsed,
                quit:           &mut should_quit,
            });

            if should_quit { break; }

            // ── 6. Physics ─────────────────────────────────────────────────
            // Apply velocity × delta_time to all transforms.
            self.world.integrate_physics(delta_time);

            // ── 7. Collision detection ──────────────────────────────────────
            // Find all overlapping pairs and emit Collision events.
            self.world.detect_collisions(&mut self.events);

            // ── 8. Late update (post-physics) ──────────────────────────────
            // Game responds to collisions: roll back solid entities, collect items.
            game.late_update(UpdateContext {
                world:          &mut self.world,
                input:          &self.input,
                mouse:          &self.mouse,
                events:         &mut self.events,
                prev_positions: &prev_positions,
                delta_time,
                elapsed,
                quit:           &mut should_quit,
            });

            if should_quit { break; }

            // ── 9. Render ──────────────────────────────────────────────────
            // Clear the back buffer, let the game fill it, then present.
            // `present()` also calls minifb's update_with_buffer, which refreshes
            // key state for the NEXT frame's input poll.
            self.renderer.clear();
            game.render(RenderContext {
                world:    &self.world,
                renderer: &mut self.renderer,
                mouse:    &self.mouse,
                delta_time,
                elapsed,
            });
            self.renderer.present()?;

            // Sync engine dimensions if the window was resized.
            if self.renderer.try_handle_resize() {
                self.width  = self.renderer.width;
                self.height = self.renderer.height;
            }

            // ── 10. Frame cap ──────────────────────────────────────────────
            // Sleep for whatever time remains to hit exactly TARGET_FPS.
            // If the frame ran long, we skip sleeping and start the next immediately.
            let frame_elapsed = Instant::now().duration_since(now);
            if frame_elapsed < FRAME_DURATION {
                std::thread::sleep(FRAME_DURATION - frame_elapsed);
            }
        }

        Ok(())
    }
}
