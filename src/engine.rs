// engine.rs — The main game engine: the game loop and the trait your game implements.

use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::platform::pump_events::EventLoopExtPumpEvents;

use crate::event::EventBus;
use crate::gamepad::GamepadState;
use crate::input::{InputManager, Key};
use crate::level::LevelData;
use crate::math::Vec2;
use crate::mouse::{MouseState, MouseButton};
use crate::renderer::{Renderer, AssetManager};
use crate::world::{EntityId, World};
use crate::project::{GameplayLoop, StartResult};

// ── Mode transition ───────────────────────────────────────────────────────────

pub enum Transition {
    /// Switch to play mode with new level data (replaces current state).
    ToPlay(LevelData),
    /// Load a saved game session (replaces current state).
    LoadGame(crate::save::SaveState),
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
    pub delta_time: f32,
    pub elapsed: f32,
    pub quit: &'a mut bool,
    pub turn_triggered: &'a mut bool,
    pub viewport_width: usize,
    pub viewport_height: usize,
    pub persistent: &'a mut HashMap<String, rhai::Dynamic>,
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
    pub persistent: &'a HashMap<String, rhai::Dynamic>,
}

// ── GameState trait ───────────────────────────────────────────────────────────

pub trait GameState {
    fn on_start(&mut self, _world: &mut World, _events: &mut EventBus, _viewport_width: usize, _viewport_height: usize) {}
    fn on_stop(&mut self, _world: &mut World, _events: &mut EventBus) {}
    fn on_pause(&mut self) {}
    fn on_resume(&mut self, _world: &mut World, _events: &mut EventBus, _viewport_width: usize, _viewport_height: usize) {}

    fn update(&mut self, ctx: UpdateContext);
    fn late_update(&mut self, _ctx: UpdateContext) {}
    fn render(&mut self, ctx: RenderContext);
    fn take_transition(&mut self) -> Option<Transition> { None }
}

// ── Engine ────────────────────────────────────────────────────────────────────

pub struct Engine {
    pub renderer: Renderer,
    pub event_loop: EventLoop<()>,
    pub gameplay_loop: GameplayLoop,
    pub assets:   AssetManager,
    pub world:    World,
    pub input:    InputManager,
    pub mouse:    MouseState,
    pub gamepad:  GamepadState,
    pub events:   EventBus,
    pub width: usize,
    pub height: usize,
    pub persistent: HashMap<String, rhai::Dynamic>,
    
    state_stack: Vec<Box<dyn GameState>>,
    simulation_accumulator: f32,
}

impl Engine {
    pub fn new(width: usize, height: usize, title: &str) -> io::Result<Self> {
        let event_loop = EventLoop::new().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        let renderer = Renderer::new(width, height, title, &event_loop)?;

        Ok(Engine {
            renderer,
            event_loop,
            gameplay_loop: GameplayLoop::RealTime,
            assets: AssetManager::new(),
            world:  World::new(),
            input:  InputManager::new(),
            mouse:  MouseState::new(),
            gamepad: GamepadState::new(),
            events: EventBus::new(),
            width,
            height,
            persistent: HashMap::new(),
            state_stack: Vec::new(),
            simulation_accumulator: 0.0,
        })
    }

    pub fn push_state(&mut self, mut state: Box<dyn GameState>) {
        if let Some(top) = self.state_stack.last_mut() {
            top.on_pause();
        }
        state.on_start(&mut self.world, &mut self.events, self.width, self.height);
        self.state_stack.push(state);
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
        self.world  = World::new();
        self.events = EventBus::new();
    }

    fn poll_events(&mut self) {
        self.input.clear();
        self.mouse.clear();
        self.gamepad.clear();

        let input = &mut self.input;
        let mouse = &mut self.mouse;
        self.gamepad.poll();
        let renderer = &mut self.renderer;
        let engine_width = &mut self.width;
        let engine_height = &mut self.height;

        let _ = self.event_loop.pump_events(Some(Duration::ZERO), |event, _| {
            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => input.quit_requested = true,
                    WindowEvent::KeyboardInput { event: key_event, .. } => {
                        // 1. Physical key for state tracking (held/pressed)
                        if let Some(key) = Key::from_winit(key_event.physical_key) {
                            if key_event.state.is_pressed() { input.handle_pressed(key); } 
                            else { input.handle_released(key); }
                        }

                        // 2. Logical key for text entry (characters, symbols, etc.)
                        if key_event.state.is_pressed() {
                            if let winit::keyboard::Key::Character(text) = &key_event.logical_key {
                                for ch in text.chars() {
                                    if !ch.is_control() {
                                        input.text_buffer.push(ch);
                                    }
                                }
                            }
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        let scale = renderer.scale_factor();
                        mouse.handle_move(position.x as f32 / scale, position.y as f32 / scale);
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        let btn = MouseButton::from_winit(button);
                        if state.is_pressed() { mouse.handle_pressed(btn); } 
                        else { mouse.handle_released(btn); }
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        match delta {
                            winit::event::MouseScrollDelta::LineDelta(x, y) => mouse.handle_scroll(x, y),
                            winit::event::MouseScrollDelta::PixelDelta(pos) => mouse.handle_scroll(pos.x as f32 / 8.0, pos.y as f32 / 16.0),
                        }
                    }
                    WindowEvent::Resized(_) => {
                        if renderer.try_handle_resize() {
                            *engine_width = renderer.width;
                            *engine_height = renderer.height;
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        });
    }

    /// Main engine execution loop.
    ///
    /// NOTE: At least one state MUST be pushed to the stack (via `push_state`) 
    /// before calling this, or it will return `Ok(None)` immediately.
    pub fn run(&mut self) -> io::Result<Option<Transition>> {
        let start_time = Instant::now();
        let mut last_frame = Instant::now();
        let mut should_quit = false;

        loop {
            self.poll_events();
            if self.input.quit_requested { return Ok(Some(Transition::Quit)); }

            let now = Instant::now();
            let delta_time = now.duration_since(last_frame).as_secs_f32();
            let elapsed    = now.duration_since(start_time).as_secs_f32();
            last_frame = now;

            self.simulation_accumulator += delta_time;

            // Only update the top-most state
            if let Some(state) = self.state_stack.last_mut() {
                if self.gameplay_loop == GameplayLoop::RealTime {
                    let mut steps = 0u32;
                    while self.simulation_accumulator >= SIM_DT && steps < MAX_SIM_STEPS {
                        steps += 1;
                        self.events.clear();
                        let prev_positions = self.world.snapshot_positions();
                        let mut turn_triggered = false;

                        state.update(UpdateContext {
                            world:           &mut self.world,
                            input:           &mut self.input,
                            mouse:           &self.mouse,
                            gamepad:         &self.gamepad,
                            events:          &mut self.events,
                            prev_positions:  &prev_positions,
                            delta_time:      SIM_DT,
                            elapsed,
                            quit:            &mut should_quit,
                            turn_triggered:  &mut turn_triggered,
                            viewport_width:  self.width,
                            viewport_height: self.height,
                            persistent:      &mut self.persistent,
                        });

                        if should_quit { return Ok(Some(Transition::Quit)); }
                        self.world.integrate_physics(SIM_DT);
                        self.world.detect_collisions(&mut self.events);

                        state.late_update(UpdateContext {
                            world:           &mut self.world,
                            input:           &mut self.input,
                            mouse:           &self.mouse,
                            gamepad:         &self.gamepad,
                            events:          &mut self.events,
                            prev_positions:  &prev_positions,
                            delta_time:      SIM_DT,
                            elapsed,
                            quit:            &mut should_quit,
                            turn_triggered:  &mut turn_triggered,
                            viewport_width:  self.width,
                            viewport_height: self.height,
                            persistent:      &mut self.persistent,
                        });

                        self.simulation_accumulator -= SIM_DT;
                        if should_quit { return Ok(Some(Transition::Quit)); }
                    }
                    if steps >= MAX_SIM_STEPS { self.simulation_accumulator = 0.0; }
                } else {
                    self.events.clear();
                    let prev_positions = self.world.snapshot_positions();
                    let mut turn_triggered = false;

                    state.update(UpdateContext {
                        world:           &mut self.world,
                        input:           &mut self.input,
                        mouse:           &self.mouse,
                        gamepad:         &self.gamepad,
                        events:          &mut self.events,
                        prev_positions:  &prev_positions,
                        delta_time,
                        elapsed,
                        quit:            &mut should_quit,
                        turn_triggered:  &mut turn_triggered,
                        viewport_width:  self.width,
                        viewport_height: self.height,
                        persistent:      &mut self.persistent,
                    });

                    if !should_quit && turn_triggered {
                        self.world.integrate_physics(1.0);
                        self.world.detect_collisions(&mut self.events);

                        state.late_update(UpdateContext {
                            world:           &mut self.world,
                            input:           &mut self.input,
                            mouse:           &self.mouse,
                            gamepad:         &self.gamepad,
                            events:          &mut self.events,
                            prev_positions:  &prev_positions,
                            delta_time,
                            elapsed,
                            quit:            &mut should_quit,
                            turn_triggered:  &mut turn_triggered,
                            viewport_width:  self.width,
                            viewport_height: self.height,
                            persistent:      &mut self.persistent,
                        });
                    }
                    self.simulation_accumulator = 0.0;
                }
            }

            if should_quit { return Ok(Some(Transition::Quit)); }

            // Render all states from bottom to top
            self.renderer.clear();
            for state in &mut self.state_stack {
                state.render(RenderContext {
                    world:    &self.world,
                    renderer: &mut self.renderer,
                    assets:   &mut self.assets,
                    mouse:    &self.mouse,
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
                        Transition::Push(new_state) => { self.push_state(new_state); }
                        Transition::Pop => { self.pop_state(); if self.state_stack.is_empty() { return Ok(None); } }
                        Transition::Quit => { return Ok(Some(Transition::Quit)); }
                        other => { return Ok(Some(other)); } // Handle ToPlay, ToEditor etc externally for now
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
