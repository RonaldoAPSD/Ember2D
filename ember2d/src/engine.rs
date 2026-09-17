// engine.rs — The main game engine: the game loop and the trait your game implements.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::time::{Duration, Instant};

use winit::event_loop::EventLoop;
use winit::keyboard::ModifiersState;
use winit::platform::pump_events::EventLoopExtPumpEvents;

use crate::audio::AudioEngine;
use crate::gamepad::GamepadState;
use crate::input::InputManager;
use crate::mouse::MouseState;
use crate::project::{GameplayLoop, StartResult};
use crate::renderer::{AssetManager, Renderer};
use crate::sim;
use ember2d_sim::event::EventBus;
use ember2d_sim::level::LevelData;
use ember2d_sim::math::Vec2;
use ember2d_sim::scripting::LogEntry;
use ember2d_sim::world::{EntityId, World};

// R76 (docs/ember2d-master-plan.md §3.2): the winit `ApplicationHandler`
// shims (`WindowInit`/`EventPump`) split out to their own file — see
// engine/window.rs's own header comment for why they're two structs, not
// one, and why this is a pure file split with no behavior change.
mod window;
use window::{EventPump, WindowInit};

// ── Mode transition ───────────────────────────────────────────────────────────

/// The five app-level variants (`ToPlay`..`ToStart`) are NOT handled by
/// `Engine::run` itself — it returns them to the caller (`ember2d-app/src/
/// app.rs`), which decides what the stack does next. In particular `ToPlay`
/// does NOT replace the editor: `app.rs` pushes the new `PlayState` ON TOP of
/// the still-live `EditorState` (so its grid/undo/panels survive the preview)
/// and `ToEditor` pops back down to it — see R51 (docs/ember2d-master-plan.md
/// §3.2) for what that stacking meant for rendering.
pub enum Transition {
    /// Switch to play mode with new level data (stacked over the editor).
    ToPlay(LevelData),
    /// Load a saved game session (replaces the current play state).
    LoadGame(ember2d_sim::save::SaveState),
    /// Return to editor (pops everything above it).
    ToEditor,
    /// Open editor with a specific project/level result.
    ToEditorWithResult(StartResult),
    /// Return to start screen (pops the whole stack).
    ToStart,
    /// Push a new state on top of the stack (e.g. Pause Menu).
    Push(Box<dyn GameState>),
    /// Pop the top state from the stack.
    Pop,
    /// Exit the application entirely.
    Quit,
}

// ── Frame rate ────────────────────────────────────────────────────────────────

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
    /// `Engine`'s own `AudioEngine` (Step 7.5-11, docs/ember2d-master-
    /// plan.md §5.6, R30) — one device stream for the app's entire
    /// lifetime, not recreated every time a `GameState` like `PlayState`
    /// is (a level transition destroys and rebuilds it). Borrowed, not
    /// owned, for the same reason `world`/`input`/every other field here
    /// is: `Engine` keeps it across frames; a `GameState` only gets it for
    /// the duration of one `update`/`late_update` call.
    pub audio: &'a mut AudioEngine,
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
    /// Whether the states BENEATH this one should still be drawn each frame
    /// (R51, docs/ember2d-master-plan.md §3.2). Default `false`: an ordinary
    /// state fully covers whatever is under it, so the engine draws nothing
    /// below it — `PlayState` over the editor, the editor over nothing. Only
    /// a genuine overlay (`PauseMenuState`: a small centered panel that needs
    /// the play screen visible around it) returns `true`. See
    /// `state_stack::render_start_index` for how the engine uses this.
    fn is_overlay(&self) -> bool {
        false
    }
    fn take_transition(&mut self) -> Option<Transition> {
        None
    }

    /// Drains this state's own script log for the caller to hand to
    /// whatever comes next on the stack (7C-7, master plan §5.3, R18) —
    /// needed because `Engine`'s stack stores `Box<dyn GameState>` with no
    /// downcasting, so `app.rs` can't reach a popped state's concrete
    /// fields directly (`PlayState::take_log`, in this case) once
    /// `pop_state` has handed it back as a trait object. Default no-op:
    /// only a state that actually keeps a script log (`PlayState`) needs
    /// to override this.
    fn take_script_log(&mut self) -> Vec<LogEntry> {
        Vec::new()
    }
    /// Receives a script log drained from another, now-popped state (7C-7,
    /// master plan §5.3, R18) — e.g. `EditorState` after F5 returns, so a
    /// script error the player triggered during preview reaches the
    /// editor's own console instead of vanishing with the `PlayState` that
    /// logged it. Default no-op: only a state with somewhere to put it
    /// (`EditorState`) needs to override this.
    fn receive_script_log(&mut self, _entries: Vec<LogEntry>) {}
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
    /// Step 7.5-11 (docs/ember2d-master-plan.md §5.6, R30): moved here from
    /// `PlayState` so the underlying device stream survives a level
    /// transition (which destroys and recreates `PlayState` entirely) —
    /// see `audio.rs`'s own header comment for the full reasoning. Threaded
    /// into whichever `GameState` is running via `UpdateContext::audio`
    /// (`sim::step`'s two construction sites).
    pub audio: AudioEngine,
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

    /// The keyboard modifier keys (Ctrl/Shift/Alt/Super) currently held —
    /// see `EventPump::modifiers`'s own doc comment (R24, 7B-4) for why
    /// this lives here rather than on `EventPump` itself.
    modifiers: ModifiersState,

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
            audio: AudioEngine::new(),
            width,
            height,
            persistent: BTreeMap::new(),
            prev_positions_buf: HashMap::new(),
            modifiers: ModifiersState::empty(),
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

    /// The top of the state stack, mutably (7C-7, master plan §5.3, R18) —
    /// lets a caller reach the current state's `GameState` methods (e.g.
    /// `receive_script_log`) without needing to hold onto the concrete
    /// value itself, which `push_state`'s `Box<dyn GameState>` already
    /// erased.
    pub fn top_state_mut(&mut self) -> Option<&mut (dyn GameState + '_)> {
        match self.state_stack.last_mut() {
            Some(s) => Some(s.as_mut()),
            None => None,
        }
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
            modifiers: &mut self.modifiers,
        };
        let _ = self.event_loop.pump_app_events(Some(Duration::ZERO), &mut pump);

        // R55 (7C-5 follow-up, docs/ember2d-master-plan.md §5.1): used to
        // also call `self.input.finish_frame_text_capture()` here — see
        // `run()`'s own comment on why that's wrong and where the call
        // moved instead.
    }

    /// Main engine execution loop.
    ///
    /// NOTE: At least one state MUST be pushed to the stack (via `push_state`)
    /// before calling this, or it will return `Ok(None)` immediately.
    ///
    /// R23 (7B-4, docs/ember2d-master-plan.md §5.2/§3): this loop used to
    /// also `thread::sleep` at the tail of every iteration to hold to
    /// `TARGET_FPS`, on top of the GPU present call already blocking on
    /// vsync (`wgpu::PresentMode::Fifo`, renderer/mod.rs) — two independent
    /// pacing mechanisms racing each other, whichever was slightly slower
    /// each frame. Fifo alone already paces the loop correctly (it blocks
    /// `Renderer::present` until the next vblank), so the sleep just added
    /// variable extra latency on top for no benefit — removed.
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
                            &mut self.audio,
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
                    // R55 (7C-5 follow-up, docs/ember2d-master-plan.md
                    // §5.1): used to be an unconditional call inside
                    // `poll_events` (once per REAL frame) instead of here
                    // (once per completed step, when one actually ran).
                    // `begin_text_capture` — the only thing that can
                    // renew a capture request — only ever runs inside
                    // `update()`, i.e. inside a step; on any display
                    // faster than 60Hz, the accumulator above legitimately
                    // produces real frames with `steps == 0` (not enough
                    // real time has accumulated for a full `SIM_DT` yet).
                    // Calling `finish_frame_text_capture` on one of those
                    // frames anyway found nothing renewed it (nothing
                    // COULD have, no step ran) and wiped `text_buffer` —
                    // discarding whatever `poll_events` just captured from
                    // real keystrokes typed during that same light frame.
                    // At a typical 144Hz refresh, roughly half of all
                    // frames run zero steps, so this silently dropped
                    // scattered characters out of ordinary fast typing —
                    // found live by the user testing the script editor
                    // this step's own fix just made reachable.
                    if steps > 0 {
                        self.input.finish_frame_text_capture();
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
                        &mut self.audio,
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
                    // R55 (docs/ember2d-master-plan.md §5.1): turn-based
                    // mode always runs exactly one step per frame (above,
                    // unconditionally), so `finish_frame_text_capture` is
                    // always paired with a step here too — see the
                    // RealTime branch's own note for why that pairing is
                    // the actual fix, not just where the call moved to.
                    self.input.finish_frame_text_capture();
                }
            }

            // Age the input buffer by real wall-clock time, once per frame,
            // regardless of how many (or how few) simulation steps ran above.
            // A press that no step claimed stays buffered for a future frame
            // until INPUT_BUFFER_WINDOW runs out.
            self.input.decay(delta_time);
            self.mouse.decay(delta_time);
            self.gamepad.decay(delta_time);

            // Render from the topmost OPAQUE state up — not every state from
            // the bottom (R51, docs/ember2d-master-plan.md §3.2). This loop
            // used to draw the whole stack bottom-to-top, which was only ever
            // invisible because `PlayState::render` opened with an opaque
            // full-screen fill that buried the paused `EditorState` beneath
            // it; 7B-3 replaced that fill with the GPU clear (which runs once,
            // HERE, before anything draws) and the editor's chrome started
            // showing through everywhere play drew nothing. `update` has
            // always been top-only (above); render now matches, except for a
            // state that opts in as an overlay (`GameState::is_overlay`).
            self.renderer.clear();
            let first = crate::state_stack::render_start_index(&self.state_stack);
            for state in &mut self.state_stack[first..] {
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
        }
    }
}
