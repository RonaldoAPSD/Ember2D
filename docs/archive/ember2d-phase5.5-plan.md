# Ember2D — Phase 5.5: Close the Netcode Seams, Add the Animation Queue

**Runs between Phase 5 and Phase 6** of `docs/ember2d-refactor-plan.md`.
**Supersedes** the earlier "Part A / Part B / Part C" draft — see §0.2 for what changed and why.

---

## 0. Why this phase exists

### 0.1 What Phase 5 left open

Phase 5 is committed (`a016267`) and delivered the scheduler, the determinism pass, the workspace split, and the replay test. Two of its three stated netcode seams (§5.4 of the refactor plan) are only partially real. Verified against current code, not assumed:

**Seam 1 — headless, device-free sim step.** `ember2d/src/sim.rs::step()` takes `&mut InputManager`, `&mut MouseState`, `&mut GamepadState` — all winit/gilrs-backed. Its own header comment admits it is "scaffolding toward the real `Simulation::step()` a later Phase 5 step builds… not that seam itself." That step never happened.

**Seam 2 — command-driven input.** `ember2d_sim::command::Command` exists and is genuinely device-free, but it only flows *internally*: a script's `on_input` calls `ctx.submit()`, which lands in `ScriptState::pending_commands` and is read back by that same step's `on_turn`. Nothing external can hand a step a `&[Command]` list — which is exactly why `tests/replay.rs` records raw key names instead of Commands, per that file's own header.

**Also missing:** the animation queue listed in Phase 5's own deliverables (`docs/ember2d-refactor-plan.md` §4.6, structural requirement 2) never landed.

**Also missing:** no CI. `.github/` is absent; the replay determinism test is a documented manual gate.

### 0.2 What changed from the earlier draft

The earlier draft's **Part B un-gated `World::integrate_physics` in turn-based mode**, so it would run every real frame. That is dropped, and replaced by the animation queue (Part 3 here).

Reasoning: physics running per *real frame* while turns stay discrete makes distance-travelled-per-turn a function of framerate. Two machines at different framerates diverge — which contradicts §5.2's determinism requirements and undermines the foundation Phase 9 rests on. The earlier draft acknowledged this and filed it as "a content/design concern, not an engine defect"; that understates it.

More importantly, it solves the wrong problem. Motion between turns in a tactical RPG — an arcing projectile, knockback — is **presentation**, not simulation. XCOM and Fire Emblem both resolve grid state instantly and play the visual over real time. That is precisely what the animation queue is for, and the plan already specifies it. Building both would give the engine two competing answers to "who owns motion between turns."

**Generality is not blocked by this.** Realtime mode already has continuous physics on a proper fixed-timestep accumulator — platformers, shooters, and bullet-hell all work today. What the earlier Part B enabled was narrowly *continuous velocity inside turn-based mode*, which the animation queue covers better.

If a future project genuinely needs simulated velocity between turns, add it then, integrating a **fixed amount per turn** rather than per frame. Deterministic, same capability.

### 0.3 Order, and why CI comes first

**Part 1 (CI) → Part 2 (Simulation) → Part 3 (animation queue).**

CI was last in the earlier draft. It should be first: Part 2 is the riskiest refactor since Phase 2, and its own highest-risk step is flagged as one where "behavior changes would be silent." Landing the workflow first means that step gets checked on two operating systems automatically instead of by hand. CI is also the smallest and least risky piece, which makes it a good session opener.

**Each part is its own commit.** Per `CLAUDE.md`'s one-logical-change-per-commit rule, these must not land as a single commit.

### 0.4 Verified findings to preserve

These were confirmed against the code and must not be re-litigated:

- **The fix is not "make `GameState`/`UpdateContext` device-free."** The editor and start screen ride the identical `Engine::run()` → `sim::step()` path as `PlayState` and genuinely need raw device access the sim-side snapshot types can't provide: `InputManager::take_text()` for layout-correct typed characters, mouse wheel deltas, and dozens of `Key` variants beyond the ~40-key snapshot whitelist. The actual simulation logic — scripts, `TurnScheduler`, `World` mutation — lives inside `PlayState`'s methods, not in `sim::step` itself.
- **`World` stays owned by `Engine`**, borrowed `&mut World` into `Simulation`'s methods — exactly how `PlayState` already receives it. Moving ownership would break `ember2d-app`'s Editor↔Play world sharing (`app.rs` reassigns `engine.world` directly on load).
- **`resolve_exit_path`** (`ember2d/src/play.rs:28`) is imported by `ember2d-editor/src/editor/impl_state.rs:7,56` via `use ember2d::play::resolve_exit_path`. It moves into `ember2d-sim` with a `pub use` re-export kept at `ember2d::play` so that import path never changes.
- **`PlayState::take_log()`** (`play.rs:397`) has zero callers — confirmed by grep. Dead code, not an integration point.
- **`PlayState.globals` / `.clips`** are read externally only by `tests/replay.rs:89-90`. Since that harness is being rewritten to own a `Simulation` directly, these become `Simulation::globals()` / `clips()` accessors; no forwarding needed on `PlayState`.
- **`turn_triggered`** is read directly by hand-built `UpdateContext` tests (`ember2d/src/play/tests.rs:154-168,400-413`, `ember2d/tests/save_load_globals.rs:50-62`) that never go through `sim::step`. It stays in `UpdateContext`/`StepResult` as reported metadata.

---

## Part 1 — CI

### 1a. `.github/workflows/ci.yml`

No `rust-toolchain.toml` exists — clean addition. Matrix on **windows-latest and ubuntu-latest**: the whole reason for the §5.2 H1/H2 determinism work is cross-platform reproducibility for future netcode. Running the same tests on Linux while the only dev machine is Windows is close to free (no GPU, window, or audio device is touched by any of these tests) and is the most direct way to catch an H1-class regression — a stray `HashMap` — that could otherwise sit invisible on one OS for months.

```yaml
on: [push, pull_request]
jobs:
  test:
    strategy:
      matrix:
        os: [windows-latest, ubuntu-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --workspace
      - run: cargo test --workspace --lib
      - run: cargo test --test persistent_on_start --test trigger_collider_layer --test roguelike_floor1 --test roguelike_combat --test roguelike_level_integrity --test save_load_globals --test replay
```

`--lib` plus explicitly named integration tests, not bare `cargo test`, matching this project's documented convention. Add `--test external_commands` and `--test turn_animation` in Parts 2 and 3 as those tests land.

Do **not** attempt the 5×-fresh-process replay discipline in CI. One run per OS per push is the right initial scope. Note in a workflow comment, and in the regression checklist, that "run 5× independently" stays a manual pre-merge gate for sim-determinism-sensitive changes.

Skip regenerating `roguelike/*.level` in CI — that's content authoring, not a build gate.

**Done when:** a PR shows the workflow green on both operating systems.

---

## Part 2 — Extract a real, device-free `Simulation` (closes seams 1 and 2)

### 2a. New `ember2d-sim/src/simulation.rs`, no callers yet

Add `pub mod simulation;` to `ember2d-sim/src/lib.rs`.

**`Simulation` owns:** `level: LevelData`, `script_engine: ScriptEngine`, `scheduler: TurnScheduler`, `globals`, `clips`, `commands: BTreeMap<i64, Command>`, `turn_number: i64`, `camera_entity: Option<EntityId>`, `exit_targets: HashMap<EntityId, String>`, `is_loading_save: bool`. Also relocate `resolve_exit_path` here.

```rust
/// Everything one step needs from outside the simulation. A struct rather
/// than a parameter list: Phase 9 will add fields here (peer command
/// batches, authority id) and a struct means those additions don't touch
/// every call site.
pub struct StepInput<'a> {
    pub input: &'a InputSnapshot,
    pub mouse: MouseSnapshot,
    pub gamepad: &'a GamepadSnapshot,
    /// Commands injected from outside — a test harness today, a network
    /// peer in Phase 9. Merged into `self.commands` at the point
    /// `play.rs:581` snapshots today. This is the seam-2 fix.
    ///
    /// One command per actor per step: a matching actor id overwrites.
    /// Correct for `TurnModel::Alternating`; `Declared` and netcode will
    /// want a queue per actor, so revisit here rather than at the call site.
    pub external_commands: &'a [Command],
    /// Caller-supplied — `Simulation` does not own the presentation Camera.
    pub camera_origin: Vec2,
    pub sim_dt: f32,
    pub elapsed: f32,
    pub viewport_w: usize,
    pub viewport_h: usize,
}

pub struct StepOutcome {
    pub turn_triggered: bool,
    pub camera_override: Option<Vec2>,
    pub shake_state: Option<ShakeState>,
    pub particles: Vec<ParticleRequest>,
    pub pending_level: Option<LevelData>,
    pub pending_load: Option<SaveState>,
    pub logs: Vec<LogEntry>,
}

impl Simulation {
    pub fn new(level: LevelData) -> Self;
    pub fn from_save(level: LevelData,
                     globals: BTreeMap<String, rhai::Dynamic>,
                     clips: BTreeMap<String, AnimationClip>) -> Self;

    pub fn on_start(&mut self, world: &mut World, viewport_w: usize, viewport_h: usize,
                    persistent: &mut BTreeMap<String, rhai::Dynamic>) -> Vec<LogEntry>;

    pub fn step(&mut self, world: &mut World, input: StepInput<'_>,
                persistent: &mut BTreeMap<String, rhai::Dynamic>) -> StepOutcome;

    pub fn late_step(&mut self, world: &mut World, events: &EventBus,
                     prev_positions: &HashMap<EntityId, Vec2>,
                     sim_dt: f32, elapsed: f32, viewport_w: usize, viewport_h: usize,
                     persistent: &mut BTreeMap<String, rhai::Dynamic>) -> StepOutcome;

    pub fn camera_entity(&self) -> Option<EntityId>;
    /// Whose turn is up — lets a caller target `external_commands` correctly.
    pub fn current_actor(&self) -> Option<EntityId>;
    pub fn globals(&self) -> &BTreeMap<String, rhai::Dynamic>;
    pub fn clips(&self) -> &BTreeMap<String, AnimationClip>;
}
```

**Internal call order mirrors today's `PlayState::update` exactly:** advance animators → `on_input` for the scheduler's local front actor → merge `external_commands` → snapshot `turn_commands` → housekeeping `run_scripts` pass → `run_actor_turn` if the front actor has a command or is AI.

**Allocation note.** `StepOutcome` returning fresh `Vec<LogEntry>` and `Vec<ParticleRequest>` every step is per-step allocation — the exact category D11 and Phase 6 exist to reduce. Acceptable for this phase to keep the diff readable, but leave a `// Phase 6: reuse buffers or take &mut Vec out-params` comment on both fields so it isn't rediscovered later as a mystery.

**What moves where:**

| Logic today (`play.rs`) | Destination |
|---|---|
| `run_actor_turn`, `apply_script_result`, `rebuild_scheduler`, `do_on_start` (`play/spawn.rs`), `resolve_exit_path` | `Simulation` — near-verbatim; `apply_script_result`'s tail becomes `StepOutcome` fields instead of direct `self.camera_override` writes |
| `update()`'s animator-advance + `on_input`/`on_update`/`on_turn` orchestration | `Simulation::step` |
| `late_update()`'s collision-pair extraction, `resolve_solid_collision`, exit-tile resolution, `run_collisions` | `Simulation::late_step` — takes `&EventBus` / `&HashMap<EntityId, Vec2>` directly, already sim-side types |
| Camera lerp/clamp, particle movement, shake decay, `flush_audio`, FPS counter, debug overlay / HUD render | Stays in `ember2d::PlayState`, now reading `StepOutcome` instead of mutating fields inline |

**Done when:** `cargo build -p ember2d-sim` succeeds and `cargo tree -p ember2d-sim` still shows zero window/renderer/input-device deps. New code is unused at this step — allow dead-code warnings here only.

### 2b. Rewire `PlayState` to own a `Simulation`

`PlayState` keeps only presentation state: `fps`, `show_debug`, `camera`, `pixels_per_unit`, `script_log`, `pending_transition`, particles, shake.

`GameState::on_start` / `update` / `late_update` shrink to: build snapshots from `ctx.input`/`ctx.mouse`/`ctx.gamepad` (relocated, not new logic) → call `self.sim.step(...)` / `self.sim.late_step(...)` → do presentation work from the returned `StepOutcome` → set `*ctx.turn_triggered = outcome.turn_triggered`, preserving the `UpdateContext` contract unchanged.

Delete `play/spawn.rs`; its logic now lives in `Simulation`.

**Highest-risk step in this phase — behavior changes here would be silent.** Verify with both the automated tests and a manual play/editor smoke test: movement, combat, camera follow, HUD log, save/load, exit tiles.

**Done when:** `cargo build --workspace` clean and every existing test passes unchanged.

### 2c. Split the presentation remainder if needed

If `play.rs` is still over the 600-line rule after 2b, split the presentation-only remainder the way `play/render.rs` and `play/tests.rs` already split off — e.g. `play/camera.rs`.

`play.rs` was ~1,263 lines before this phase. Moving the simulation logic out should take a large bite; check where it lands and split the rest.

### 2d. Simplify `TurnHarness` (`ember2d/tests/common/mod.rs`) to drive `Simulation` directly

Drops `PlayState`, `InputManager`, `MouseState`, `GamepadState`, and the winit `Key` type entirely. Owns `world: World` + `sim: Simulation` + persistent/elapsed/viewport dims.

Public API keeps the same shape so `tests/roguelike_*.rs` need minimal changes:

- `TurnHarness::load(path)`
- `harness.press(name: &str) -> bool` — builds a single-frame `InputSnapshot { held: {name}, pressed: {name} }`. A synthetic press is a one-frame tap, matching every existing test's usage, not a held-until-released key. Calls `sim.step`, returns `turn_triggered`.
- `harness.frame() -> bool` — no input (was `frame(None)`)
- `harness.turn(name: &str) -> bool` — was `turn(key)`, same follow-up-frame draining behaviour
- `harness.player_id()` / `player_pos()` — unchanged, read `World` directly

Use the exact string names `InputSnapshot::snapshot()`'s `KEY_MAP` already produces (`"w"`, `"space"`, `"escape"`). Every `Key::X` used in `tests/roguelike_*.rs` and `tests/replay.rs` (W/A/S/D/Space/Q) already has a string form, since `on_input` scripts read them that way.

**Leave alone:** `persistent_on_start.rs`, `trigger_collider_layer.rs`, `save_load_globals.rs`, `play/tests.rs`. These construct `UpdateContext`/`PlayState` by hand and test the engine-level path; they don't go through `TurnHarness`.

**Done when:** `cargo build --workspace` clean and the roguelike tests pass with only mechanical call-site updates.

### 2e. `tests/replay.rs` migration + new `tests/external_commands.rs`

Two changes, not one — either alone leaves something unproven.

1. **Migrate `replay.rs`** to the simplified harness (mechanical: `h.play.globals` → `h.sim.globals()`). Keeps proving full-pipeline byte-identical determinism.
2. **Add `ember2d/tests/external_commands.rs`:** drive a bare `Simulation` directly — no `InputSnapshot`, no `harness.press()` — passing a hand-built `Command { actor, action: "move".into(), params: vec![...] }` into `external_commands` for the player's actor id, and assert the resulting world state matches what the equivalent keypress produces in `roguelike_floor1.rs`.

**This second test is the only thing that actually exercises seam 2.** Migration alone never calls `external_commands` with anything non-empty, so the seam would remain unproven while looking done.

Add `--test external_commands` to the CI workflow.

**Done when:** both pass individually, and `replay` passes 5× as independent fresh processes.

### 2f. Guardrail check — no code, a diff review before merging Part 2

Confirm via `git diff` that `ember2d/src/sim.rs`, `ember2d/src/engine.rs` (`GameState`/`UpdateContext`/`RenderContext`), and everything under `ember2d-editor/` are untouched. If the editor and start screen build and run unchanged, the extraction held.

---

## Part 3 — The animation queue

Replaces the earlier draft's physics un-gating. This is the Phase 5 deliverable (`docs/ember2d-refactor-plan.md` §4.6, structural requirement 2) that didn't land.

### 3a. Why this is needed regardless

A turn resolves instantly in the simulation — the whole round happens in one step. Without an animation queue the player sees no motion between pressing a key and the world being three actions further along. Today the roguelike gets away with it because everything is a single grid hop, but any turn-based game with visible action — a projectile, an enemy crossing the room, a hit reaction — needs this.

It is also the correct home for motion between turns: grid state updates instantly, the visual plays over real time. That is how XCOM and Fire Emblem work, and it is why the physics un-gating is unnecessary.

### 3b. The types

Sim-side, in `ember2d-sim`, since actions emit these and they must serialize with a save:

```rust
/// One visual event emitted by a resolved action. Durations are REAL
/// SECONDS and have no relationship to simulation time — a 150-cost
/// action may animate for 0.2s or 2.0s with identical game consequences.
/// Keeping these separate is what allows a "fast-forward animations"
/// setting later without touching game balance.
pub enum AnimationEvent {
    Move  { entity: EntityId, from: Vec2, to: Vec2, duration: f32 },
    Flash { entity: EntityId, color: Color, duration: f32 },
    Shake { entity: EntityId, duration: f32 },
}
```

Emitted into `StepOutcome.animations: Vec<AnimationEvent>`.

### 3c. Playback

Playback state is presentation and lives engine-side, in `ember2d::PlayState`. The queue drains over real frames using `frame_delta_time`, not `sim_dt`.

**The scheduler waits for the queue to drain before resolving the next turn.** That is the whole point — without it the animation is decorative and the sim races ahead.

Concretely: `PlayState::update` skips calling `self.sim.step(...)` while its animation queue is non-empty, letting render frames continue. When the queue empties, stepping resumes.

**Determinism is preserved** because the queue is presentation-only and never feeds back into the simulation. Draining faster or slower changes nothing about world state — verify this by confirming `replay.rs` still passes, since the harness drives `Simulation` directly and bypasses playback entirely.

### 3d. Script API

```rhai
ctx.animate_move(id, to_x, to_y, duration)
ctx.animate_flash(id, color, duration)
ctx.animate_shake(id, duration)
ctx.is_animating(id)
```

In realtime mode these still work but are usually unnecessary, since movement is already continuous. Additive — no `api_version` bump.

### 3e. Test

Add `ember2d/tests/turn_animation.rs`: a scripted action emits a `Move` event, and the scheduler does not advance to the next actor until the queue has drained. Add `--test turn_animation` to CI.

**Done when:** an enemy's move is visible as motion rather than a teleport in the roguelike smoke test, `replay.rs` still passes, and the scheduler demonstrably waits.

---

## 4. Documentation

- **`docs/ember2d-refactor-plan.md` §5.4** — mark seams 1 and 2 closed, in the same append-don't-rewrite style used for seam 3. Note that commands stay script-interpreted (settled decision, unchanged) and external injection is additive, not a redesign.
- **§7 Phase 5** — note `Simulation::step` now exists in `ember2d_sim::simulation`, closing the gap `sim.rs`'s Step 5d header flagged as future work; and that the animation queue landed in Phase 5.5.
- **§4.6** — record that turn-based motion is owned by the animation queue, not by physics, and why: per-frame physics integration in turn mode makes distance-per-turn framerate-dependent, which breaks §5.2 determinism. If a future project needs simulated velocity between turns, integrate a fixed amount **per turn**.
- **`docs/ember2d-scripting-api.md` §7** — move the animation functions from "planned" to live.
- **`docs/HANDOFF.md` and the regression checklist** — `TurnHarness` no longer touches `InputManager`/`MouseState`/`GamepadState`/winit `Key`; CI now runs on two operating systems; the 5×-fresh-process replay run stays a manual pre-merge gate.

---

## 5. Verification, end to end

1. `cargo build --workspace` clean; `cargo tree -p ember2d-sim` still shows zero renderer/window/input-device deps.
2. `cargo test --workspace --lib` plus every named integration test passes: `persistent_on_start`, `trigger_collider_layer`, `roguelike_floor1`, `roguelike_combat`, `roguelike_level_integrity`, `save_load_globals`, `replay`, `external_commands`, `turn_animation`.
3. `cargo test --test replay` run 5× as independent fresh processes.
4. Manual smoke test: `cargo run -- roguelike/floor1.level` (movement, combat, camera, HUD, exit — and enemy moves now animate) and `cargo run -- --editor roguelike/floor1.level` (painting, text fields, scroll/zoom, save) both behave correctly.
5. GitHub Actions green on both windows-latest and ubuntu-latest.
6. `git diff` confirms `ember2d/src/sim.rs`, `engine.rs`'s `GameState`/`UpdateContext`, and `ember2d-editor/` are untouched by Part 2.

---

## 6. Explicitly out of scope

- **Physics in turn-based mode.** See §0.2. Revisit only when a real project needs it, and then per-turn, not per-frame.
- **`TurnModel::Energy` / `ActionCost` / `Declared`.** Defined but unexposed; still correct.
- **Real transport, rollback, snapshot performance.** Phase 9.
- **The D11 allocation work**, including `StepOutcome`'s vectors. Phase 6.
- **Any editor change.** Part 2's guardrail exists specifically to prove there wasn't one.
