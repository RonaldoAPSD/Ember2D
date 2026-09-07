// engine.rs — The main game engine: the game loop and the trait your game implements.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::window::{Window, WindowId};

use crate::gamepad::GamepadState;
use crate::input::{InputManager, Key};
use crate::mouse::{MouseButton, MouseState};
use crate::project::{GameplayLoop, StartResult};
use crate::renderer::{AssetManager, Renderer};
use crate::sim;
use ember2d_sim::event::EventBus;
use ember2d_sim::level::LevelData;
use ember2d_sim::math::Vec2;
use ember2d_sim::world::{EntityId, World};

// ── Mode transition ───────────────────────────────────────────────────────────

pub enum Transition {
    /// Switch to play mode with new level data (replaces current state).
    ToPlay(LevelData),
    /// Load a saved game session (replaces current state).
    LoadGame(ember2d_sim::save::SaveState),
    /// Return to editor (replaces current state).
    ToEditor,
    /// Open editor with a specific project/level result.
    ToEditorWithResult(StartResult),
    /// Return to start screen (replaces current state).
    ToStart,
    /// Push a new state on top of the stack (e.g. Pause Menu).
    Push(Box<dyn GameState>),
    /// Pop the top state from the stack.
    Pop,
    /// Exit the application entirely.
    Quit,
}

// ── Frame rate ────────────────────────────────────────────────────────────────

const TARGET_FPS: u64 = 60;
const FRAME_DURATION: Duration = Duration::from_micros(1_000_000 / TARGET_FPS);
const SIM_DT: f32 = 1.0 / 60.0;
const MAX_SIM_STEPS: u32 = 8;

// ── Context structs ───────────────────────────────────────────────────────────

pub struct UpdateContext<'a> {
    pub world: &'a mut World,
    pub input: &'a mut InputManager,
    pub mouse: &'a MouseState,
    pub gamepad: &'a GamepadState,
    pub events: &'a mut EventBus,
    pub prev_positions: &'a HashMap<EntityId, Vec2>,
    /// The **fixed** simulation timestep — what scripts (`ctx.get_delta()`),
    /// `World::integrate_physics`, and `Animator::advance` see. Always a
    /// constant (`SIM_DT` in realtime mode; the same constant in turn-based
    /// mode as of Step 5d, docs/ember2d-phase5-plan.md — it used to be real
    /// wall-clock time there, which made scripts' own notion of "how much
    /// time passed" nondeterministic between runs). Never derive presentation
    /// timing from this in engine code — that's what `frame_delta_time` is
    /// for.
    pub delta_time: f32,
    /// The **real** wall-clock time since the last frame — for presentation
    /// state a script never reads back: camera lerp, camera-shake decay, the
    /// particle system, the F3 debug overlay's FPS counter. Added in Step 5d
    /// (docs/ember2d-phase5-plan.md) specifically so those stay visually
    /// smooth at whatever the real framerate is, without smuggling wall-clock
    /// time into anything a script or the simulation reads — see
    /// `delta_time`'s own doc comment for the boundary this maintains.
    /// Equal to `delta_time` in realtime mode (a heavy frame's several sim
    /// steps already sum to approximately the real frame time, by
    /// construction of the accumulator, so a separate value isn't needed
    /// there); only turn-based mode's value differs from `delta_time`.
    pub frame_delta_time: f32,
    pub elapsed: f32,
    pub quit: &'a mut bool,
    pub turn_triggered: &'a mut bool,
    pub viewport_width: usize,
    pub viewport_height: usize,
    pub persistent: &'a mut BTreeMap<String, rhai::Dynamic>,
}

impl<'a> UpdateContext<'a> {
    pub fn trigger_turn(&mut self) {
        *self.turn_triggered = true;
    }
}

pub struct RenderContext<'a> {
    pub world: &'a World,
    pub renderer: &'a mut Renderer,
    pub assets: &'a mut AssetManager,
    pub mouse: &'a MouseState,
    pub delta_time: f32,
    pub elapsed: f32,
    pub persistent: &'a BTreeMap<String, rhai::Dynamic>,
}

// ── GameState trait ───────────────────────────────────────────────────────────

pub trait GameState {
    /// `persistent` is the engine's real cross-level persistent store — the
    /// same map `UpdateContext::persistent` gives `update`. Passing it here
    /// (rather than a throwaway local) is what lets `ctx.set_persistent`
    /// calls made from a script's `on_start` actually survive (defect D2 in
    /// docs/ember2d-refactor-plan.md §3 — previously PlayState::on_start
    /// ran scripts against a fresh, discarded `HashMap`).
    fn on_start(
        &mut self,
        _world: &mut World,
        _events: &mut EventBus,
        _viewport_width: usize,
        _viewport_height: usize,
        _persistent: &mut BTreeMap<String, rhai::Dynamic>,
    ) {
    }
    fn on_stop(&mut self, _world: &mut World, _events: &mut EventBus) {}
    fn on_pause(&mut self) {}
    fn on_resume(
        &mut self,
        _world: &mut World,
        _events: &mut EventBus,
        _viewport_width: usize,
        _viewport_height: usize,
    ) {
    }

    fn update(&mut self, ctx: UpdateContext);
    fn late_update(&mut self, _ctx: UpdateContext) {}
    fn render(&mut self, ctx: RenderContext);
    fn take_transition(&mut self) -> Option<Transition> {
        None
    }
}

// ── winit 0.30 ApplicationHandler shims ─────────────────────────────────────
//
// 7B-1 (docs/ember2d-master-plan.md §5.2): winit 0.30 replaced direct,
// eager window creation (the old `WindowBuilder::new()...build(&event_loop)`,
// callable any time) with `ActiveEventLoop::create_window`, reachable only
// from inside an `ApplicationHandler` callback — `resumed()`, specifically,
// which fires once the platform is ready to create windows/GL contexts.
// Two small handlers, not one, because they run at two different points
// with two different jobs: `WindowInit` runs exactly once, synchronously,
// inside `Engine::new` (pumped in a loop until `resumed()` fires and hands
// back a window) so `Renderer::new` — and therefore `Engine::new` itself —
// keeps returning a fully-initialized, ready-to-use `Engine`, exactly as
// every caller already expects; `EventPump` runs every frame from
// `poll_events`, replacing the closure `pump_events` (deprecated in favor
// of `pump_app_events`) used to take, with the identical `WindowEvent`
// handling that closure had. Same `loop {}`-per-frame shape either way —
// see `Engine::run`'s own loop, unchanged by this split.

/// Exists only to receive the one `resumed()` call `Engine::new` pumps for.
struct WindowInit {
    width: usize,
    height: usize,
    title: String,
    window: Option<Arc<Window>>,
}

impl ApplicationHandler for WindowInit {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // Already created — resumed() can in principle fire again.
        }
        let pixel_width = self.width * crate::renderer::CELL_W;
        let pixel_height = self.height * crate::renderer::CELL_H;
        // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): only a starting
        // guess — no window (and therefore no authoritative
        // `window.scale_factor()`) exists yet to ask. `Renderer::new`
        // re-derives the real scale, cell grid, and `ScreenMapping` from
        // the window's actual `inner_size()`/`scale_factor()` once it
        // exists (`recompute_layout`), so a wrong guess here only costs an
        // extra resize-equivalent recompute, not a lasting mismatch — see
        // that function's own doc comment. `PhysicalSize`, not
        // `LogicalSize`: this guessed scale IS meant to end up as the
        // window's actual physical size, same as the real DPI-derived
        // `scale` will apply later; requesting a `LogicalSize` here would
        // let winit's own OS-level DPI scaling apply on top and
        // double-scale.
        let guessed_scale = event_loop
            .primary_monitor()
            .map(|m| m.scale_factor() as f32)
            .unwrap_or(crate::renderer::INITIAL_SCALE_GUESS)
            .round()
            .max(1.0);
        let attrs = Window::default_attributes().with_title(&self.title).with_inner_size(
            winit::dpi::PhysicalSize::new(
                pixel_width as f32 * guessed_scale,
                pixel_height as f32 * guessed_scale,
            ),
        );
        if let Ok(window) = event_loop.create_window(attrs) {
            self.window = Some(Arc::new(window));
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
        // Nothing to do before a window (and therefore a Renderer) exists.
    }
}

/// The real per-frame window-event handler, built fresh each `poll_events`
/// call from whichever `Engine` fields it needs to mutate — same fields
/// the old closure captured by `&mut` reference.
struct EventPump<'a> {
    input: &'a mut InputManager,
    mouse: &'a mut MouseState,
    renderer: &'a mut Renderer,
    engine_width: &'a mut usize,
    engine_height: &'a mut usize,
}

impl<'a> ApplicationHandler for EventPump<'a> {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // The window already exists by the time this pump runs (created in
        // Engine::new via WindowInit) — nothing to do even if the platform
        // re-fires resumed() (e.g. after a suspend/resume cycle).
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => self.input.quit_requested = true,
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                // 1. Physical key for state tracking (held/pressed)
                if let Some(key) = Key::from_winit(key_event.physical_key) {
                    if key_event.state.is_pressed() {
                        self.input.handle_pressed(key);
                    } else {
                        self.input.handle_released(key);
                    }
                }

                // 2. Logical key for text entry (characters, symbols, etc.)
                // R44 (7A-11, docs/ember2d-master-plan.md §5.1): was
                // `if let Key::Character(text) = ...` only — see
                // `logical_key_text`'s own doc comment for why that
                // silently dropped every Space press.
                if key_event.state.is_pressed() {
                    self.input.text_buffer.push_str(&Key::logical_key_text(&key_event.logical_key));
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): was a
                // single-axis, DPI-blind `scale_factor()` pre-division
                // that never accounted for the letterbox origin —
                // `handle_move` does the full physical->logical conversion
                // via `ScreenMapping` now.
                self.mouse.handle_move(
                    position.x as f32,
                    position.y as f32,
                    self.renderer.screen_mapping(),
                );
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let btn = MouseButton::from_winit(button);
                if state.is_pressed() {
                    self.mouse.handle_pressed(btn);
                } else {
                    self.mouse.handle_released(btn);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                winit::event::MouseScrollDelta::LineDelta(x, y) => self.mouse.handle_scroll(x, y),
                winit::event::MouseScrollDelta::PixelDelta(pos) => {
                    self.mouse.handle_scroll(pos.x as f32 / 8.0, pos.y as f32 / 16.0)
                }
            },
            WindowEvent::Resized(_) => {
                if self.renderer.try_handle_resize() {
                    *self.engine_width = self.renderer.width;
                    *self.engine_height = self.renderer.height;
                }
            }
            // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): the window
            // moved to a monitor with a different DPI scale factor (or the
            // OS scale setting changed) — re-derive `scale`/the cell grid/
            // `ScreenMapping` the same way a resize would. Ignoring
            // `inner_size_writer`: nothing here requests a specific inner
            // size in response, `recompute_layout` just reads whatever the
            // window's size/scale_factor are by the time this fires.
            WindowEvent::ScaleFactorChanged { .. } => {
                if self.renderer.handle_scale_factor_changed() {
                    *self.engine_width = self.renderer.width;
                    *self.engine_height = self.renderer.height;
                }
            }
            _ => {}
        }
    }
}

// ── Engine ────────────────────────────────────────────────────────────────────

pub struct Engine {
    pub renderer: Renderer,
    pub event_loop: EventLoop<()>,
    pub gameplay_loop: GameplayLoop,
    pub assets: AssetManager,
    pub world: World,
    pub input: InputManager,
    pub mouse: MouseState,
    pub gamepad: GamepadState,
    pub events: EventBus,
    pub width: usize,
    pub height: usize,
    /// `BTreeMap`, not `HashMap` (Step 5b, docs/ember2d-phase5-plan.md) — see
    /// `PlayState::globals`'s doc comment for why: this is the same
    /// serialize-deterministically requirement, one level up (this is what
    /// `SaveState::persistent` gets built from).
    pub persistent: BTreeMap<String, rhai::Dynamic>,

    /// Phase 6 Step 10 (docs/ember2d-phase6-plan.md): the buffer `sim::step`
    /// snapshots each step's pre-move positions into
    /// (`World::snapshot_positions_into`), owned here so it survives across
    /// frames instead of being a fresh `HashMap` allocated inside `step`
    /// every single call. `HashMap` (not `BTreeMap`) is fine here — this is
    /// presentation/scratch state consumed entirely within the frame that
    /// fills it (`UpdateContext::prev_positions`), never iterated in an
    /// order-sensitive way, unlike sim-side stores.
    prev_positions_buf: HashMap<EntityId, Vec2>,

    state_stack: Vec<Box<dyn GameState>>,
    simulation_accumulator: f32,
}

impl Engine {
    pub fn new(width: usize, height: usize, title: &str) -> io::Result<Self> {
        let mut event_loop =
            EventLoop::new().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        // 7B-1 (docs/ember2d-master-plan.md §5.2): pump until WindowInit's
        // resumed() fires and hands back a window — see that struct's own
        // doc comment. Desktop platforms (this project's only targets)
        // fire resumed() on the very first pump; looping is defensive, not
        // load-bearing, in case a platform ever needs more than one.
        let mut window_init = WindowInit { width, height, title: title.to_string(), window: None };
        while window_init.window.is_none() {
            event_loop.pump_app_events(Some(Duration::ZERO), &mut window_init);
        }
        let window = window_init.window.take().unwrap();

        // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): Renderer::new no
        // longer takes width/height — it derives the real cell grid from
        // the window itself (see that function's own doc comment). Read
        // it back here rather than trusting the `width`/`height` this
        // function was originally asked for, which `WindowInit` only ever
        // used as an initial sizing guess.
        let renderer = Renderer::new(window)?;
        let width = renderer.width;
        let height = renderer.height;

        Ok(Engine {
            renderer,
            event_loop,
            gameplay_loop: GameplayLoop::RealTime,
            assets: AssetManager::new(),
            world: World::new(),
            input: InputManager::new(),
            mouse: MouseState::new(),
            gamepad: GamepadState::new(),
            events: EventBus::new(),
            width,
            height,
            persistent: BTreeMap::new(),
            prev_positions_buf: HashMap::new(),
            state_stack: Vec::new(),
            simulation_accumulator: 0.0,
        })
    }

    pub fn push_state(&mut self, mut state: Box<dyn GameState>) {
        if let Some(top) = self.state_stack.last_mut() {
            top.on_pause();
        }
        state.on_start(
            &mut self.world,
            &mut self.events,
            self.width,
            self.height,
            &mut self.persistent,
        );
        self.state_stack.push(state);
        // R19 (7A-2, docs/ember2d-master-plan.md): the deepest legitimate
        // stack today is EditorState -> PlayState -> PauseMenuState (3). A
        // 4th means something (like the ToStart orphan this step fixes)
        // failed to pop before pushing again — catch that in debug builds
        // rather than let the stack grow unboundedly across state changes.
        debug_assert!(self.state_stack.len() <= 3, "editor state stack depth exceeded 3 ({}) — a Transition handler is missing a pop_state", self.state_stack.len());
    }

    pub fn pop_state(&mut self) -> Option<Box<dyn GameState>> {
        let mut old = self.state_stack.pop();
        if let Some(ref mut s) = old {
            s.on_stop(&mut self.world, &mut self.events);
        }
        if let Some(top) = self.state_stack.last_mut() {
            top.on_resume(&mut self.world, &mut self.events, self.width, self.height);
        }
        old
    }

    pub fn reset_world(&mut self) {
        self.world = World::new();
        self.events = EventBus::new();
    }

    pub fn state_stack_len(&self) -> usize {
        self.state_stack.len()
    }

    fn poll_events(&mut self) {
        self.input.clear();
        self.mouse.clear();
        self.gamepad.clear();
        self.gamepad.poll();

        // 7B-1 (docs/ember2d-master-plan.md §5.2): pump_events deprecated
        // in favor of pump_app_events, which wants an ApplicationHandler
        // rather than a closure — see EventPump's own doc comment above.
        let mut pump = EventPump {
            input: &mut self.input,
            mouse: &mut self.mouse,
            renderer: &mut self.renderer,
            engine_width: &mut self.width,
            engine_height: &mut self.height,
        };
        let _ = self.event_loop.pump_app_events(Some(Duration::ZERO), &mut pump);

        // R12 (7A-2, docs/ember2d-master-plan.md): the other half of
        // InputManager's text-capture mechanism — see
        // `text_capture_requested`'s own doc comment (input.rs). Whichever
        // widget wants this frame's `text_buffer` must have called
        // `begin_text_capture` during last frame's update (the only point
        // in the loop before this one that could have); if nothing did,
        // clear it now rather than let it silently carry into whatever
        // becomes focused next.
        self.input.finish_frame_text_capture();
    }

    /// Main engine execution loop.
    ///
    /// NOTE: At least one state MUST be pushed to the stack (via `push_state`)
    /// before calling this, or it will return `Ok(None)` immediately.
    pub fn run(&mut self) -> io::Result<Option<Transition>> {
        let start_time = Instant::now();
        let mut last_frame = Instant::now();

        loop {
            self.poll_events();
            if self.input.quit_requested {
                return Ok(Some(Transition::Quit));
            }

            let now = Instant::now();
            let delta_time = now.duration_since(last_frame).as_secs_f32();
            let elapsed = now.duration_since(start_time).as_secs_f32();
            last_frame = now;

            self.simulation_accumulator += delta_time;

            // Only update the top-most state. The actual per-step sequence
            // (consume input, update, then conditionally
            // physics/collisions/late_update) lives in `sim::step` now —
            // see that module's header comment for why (Step 5d,
            // docs/ember2d-phase5-plan.md): this loop and
            // `tests/common/mod.rs`'s `TurnHarness` used to hand-duplicate
            // it, which is exactly the kind of divergence-between-copies
            // hazard `docs/HANDOFF.md` warns about.
            if let Some(state) = self.state_stack.last_mut() {
                if self.gameplay_loop == GameplayLoop::RealTime {
                    let mut steps = 0u32;
                    while self.simulation_accumulator >= SIM_DT && steps < MAX_SIM_STEPS {
                        steps += 1;
                        // frame_dt == sim_dt (SIM_DT) here, deliberately: a
                        // heavy frame's several fixed-SIM_DT steps already
                        // sum to approximately the real frame time, by
                        // construction of this accumulator, so presentation
                        // code (camera lerp, shake, the FPS counter) doesn't
                        // need a separate real-time signal in realtime mode
                        // — see `UpdateContext::frame_delta_time`'s own doc
                        // comment. `gate_late_phase_on_turn: false` — the
                        // late phase (physics/collisions/late_update) always
                        // runs every step in realtime mode, unconditionally.
                        let result = sim::step(
                            state.as_mut(),
                            &mut self.world,
                            &mut self.input,
                            &mut self.mouse,
                            &mut self.gamepad,
                            &mut self.events,
                            &mut self.persistent,
                            &mut self.prev_positions_buf,
                            SIM_DT,
                            SIM_DT,
                            SIM_DT,
                            elapsed,
                            self.width,
                            self.height,
                            false,
                        );
                        if result.should_quit {
                            return Ok(Some(Transition::Quit));
                        }
                        self.simulation_accumulator -= SIM_DT;
                    }
                    if steps >= MAX_SIM_STEPS {
                        self.simulation_accumulator = 0.0;
                    }
                } else {
                    // Turn-based mode still only runs one step per frame,
                    // but a buffered press may have been waiting several
                    // frames for the turn to come around — `sim::step`'s own
                    // `consume_step` is what claims it. See INPUT_BUFFER_WINDOW.
                    //
                    // sim_dt is the fixed SIM_DT here, not the real
                    // `delta_time` (Step 5d fix, docs/ember2d-phase5-plan.md)
                    // — scripts/physics/animators must see a deterministic
                    // timestep even though turn-based mode only advances on
                    // player action, or a future replay can't reproduce a
                    // run exactly. `delta_time` (real wall-clock) still
                    // flows through as frame_dt, for the camera lerp/shake/
                    // FPS counter that must stay visually smooth regardless.
                    // `gate_late_phase_on_turn: true` — the late phase only
                    // runs if this step actually resolved an actor's turn
                    // (`PlayState::run_actor_turn`'s `TurnScheduler`-driven
                    // decision, Step 5f — there's no more script-callable
                    // `ctx.trigger_turn()`).
                    let result = sim::step(
                        state.as_mut(),
                        &mut self.world,
                        &mut self.input,
                        &mut self.mouse,
                        &mut self.gamepad,
                        &mut self.events,
                        &mut self.persistent,
                        &mut self.prev_positions_buf,
                        SIM_DT,
                        delta_time,
                        1.0,
                        elapsed,
                        self.width,
                        self.height,
                        true,
                    );
                    if result.should_quit {
                        return Ok(Some(Transition::Quit));
                    }
                    self.simulation_accumulator = 0.0;
                }
            }

            // Age the input buffer by real wall-clock time, once per frame,
            // regardless of how many (or how few) simulation steps ran above.
            // A press that no step claimed stays buffered for a future frame
            // until INPUT_BUFFER_WINDOW runs out.
            self.input.decay(delta_time);
            self.mouse.decay(delta_time);
            self.gamepad.decay(delta_time);

            // Render all states from bottom to top
            self.renderer.clear();
            for state in &mut self.state_stack {
                state.render(RenderContext {
                    world: &self.world,
                    renderer: &mut self.renderer,
                    assets: &mut self.assets,
                    mouse: &self.mouse,
                    delta_time,
                    elapsed,
                    persistent: &self.persistent,
                });
            }
            self.renderer.present()?;

            // Process transitions from top-most state
            if let Some(state) = self.state_stack.last_mut() {
                if let Some(t) = state.take_transition() {
                    match t {
                        Transition::Push(new_state) => {
                            self.push_state(new_state);
                        }
                        Transition::Pop => {
                            self.pop_state();
                            if self.state_stack.is_empty() {
                                return Ok(None);
                            }
                        }
                        Transition::Quit => {
                            return Ok(Some(Transition::Quit));
                        }
                        other => {
                            return Ok(Some(other));
                        } // Handle ToPlay, ToEditor etc externally for now
                    }
                }
            } else {
                return Ok(None); // Stack empty
            }

            let frame_elapsed = Instant::now().duration_since(now);
            if frame_elapsed < FRAME_DURATION {
                std::thread::sleep(FRAME_DURATION - frame_elapsed);
            }
        }
    }
}
