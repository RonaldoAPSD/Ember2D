# Ember2D — Refactor Plan

**Written against:** the `gemini` branch, v0.5.0
**Supersedes:** the earlier version of this plan, which was written against `main` (v0.3.4) and was wrong about roughly half the codebase
**Companions:** `ember2d-regression-checklist.md`, `ember2d-scripting-api.md`

---

## 1. Goals

1. **One renderer, world-space.** ASCII is not a mode. A glyph is a textured quad; a sprite is a textured quad. Both go through the same pipeline, and an ASCII project can place sprites in the same scene.
2. **General purpose.** No engine code branches on project type. Every capability is available to every project.
3. **Two time models.** Realtime and turn-based, switchable in project settings.
4. **Full in-engine authoring.** Levels, scripts, assets, animation — without leaving the editor.
5. **Multiplayer-ready seams.** Netcode is not built here, but the simulation is shaped so it can be added later (§5).
6. **Learning is a first-class goal** — except where a dependency removes work that has already caused burnout once (§6).

### Non-goals
3D · physics beyond AABB · shipping netcode · mobile/console · API backwards compatibility (0.x, one user — break cleanly).

---

## 2. Where the code actually is

`gemini` is two minor versions ahead of `main` and far more capable than the previous plan assumed.

### Already built

| Area | State |
|---|---|
| Window & input | `winit` 0.29 via `pump_events`, so the loop stays a normal `loop {}`. Own `Key` and `MouseButton` enums. Gamepad support through `gilrs`. |
| GPU | `wgpu` 0.19. `RenderBackend` trait, `WgpuBackend`, instanced quads, batching by texture, font atlas generated at startup, texture cache, scissor rects, dynamic instance buffer growth. |
| Engine loop | Single `run()`. Fixed-timestep accumulator (`SIM_DT`, `MAX_SIM_STEPS = 8`) for realtime; turn mode gated on `turn_triggered`. State stack with push/pop/pause/resume. |
| World | Full `Serialize`/`Deserialize`. Entity **parenting** with `get_global_position`. Collision layers + masks. |
| Save system | `SaveState` — world + persistent globals + level path, round-tripped through RON. |
| Scripting | ~95 registered functions across `scripting/api.rs`. Includes globals, persistence, timers, RNG, spatial queries, raycast, A\* pathfinding, camera, mouse, gamepad, HUD widgets, particles, spatial audio, save/load. |
| Project settings | `visual_style` (ClassicASCII/Sprites2D), `gameplay_loop` (RealTime/TurnBased), `start_level` — all persisted and wired in `main.rs`. |
| Editor | Layer-aware grid (`HashMap<(x,y,layer)>`), float zoom, smooth scroll, HSV colour picker, context menus, modals, in-engine script editor, savable palette, native file dialogs via `rfd`. |

**This means the old plan's Phase 3 (winit), most of Phase 4 (wgpu), all of Phase 0.5 (scripting API), the realtime half of Phase 6, and the loop-dedup cleanup are done.** Open question 5 from the old plan — scene graph vs flat list — is answered: you have a hierarchy.

### Not built

**Everything above wgpu still thinks in character cells.**

- `draw_char(x: usize, y: usize)` takes cell coordinates. The ortho projection is `0..width` in cells. There is no world-space draw call and no camera inside the renderer — `PlayState` computes an integer camera offset and subtracts it before drawing.
- `Sprite` is glyph-primary. `texture: Option<String>` is a path re-resolved every draw. The call site is `draw_texture(col * 8, row * 16, t, 32.0)` — magic scale, no src rect, no size, no rotation, tint forced to white, and texture sprites skip the bounds culling glyphs get.
- Animation is `frames: Vec<char>` only. No sprite-sheet frames.
- `VisualStyle::Sprites2D` exists as an enum variant. Nothing constructs a real 2D pipeline behind it.

### Two rendering details that will bite

- **Colour space mismatch.** The font atlas is `Rgba8UnormSrgb`, loaded textures are `Rgba8Unorm`, and the surface deliberately selects a non-sRGB format. Glyph and sprite colours will not match.
- **Batching is order-dependent.** `ensure_batch` only merges *consecutive* same-texture runs. A z-sorted list alternating glyphs and textures degenerates to one draw call per sprite.

---

## 3. Defects found in review

Concrete bugs, not design opinions. Phase 1 clears most of them.

| # | Defect | Location |
|---|---|---|
| D1 | **Input edge detection breaks under fixed timestep.** `poll_events` clears input once per frame, but the accumulator can run `update` up to 8 times that frame with the same `just_pressed` set — *and* zero times on a light frame, silently dropping the press entirely. Duplicates and drops, both directions. **Resolution: buffer until consumed (§4.1).** | `engine.rs` |
| D2 | **`set_persistent` in `on_start` is discarded.** `do_on_start` passes a local `HashMap` commented "Dummy" and drops it. | `play/spawn.rs` |
| D3 | **RNG is nondeterministic in three places.** `SmallRng::from_entropy()` for scripts, plus a fresh entropy-seeded RNG allocated *every frame* for particles and again for camera shake. | `scripting/engine.rs`, `play.rs` |
| D4 | **Trigger colliders default to layer `"solid"`** when `collider_layer` is empty, corrupting mask filtering and `is_solid_at`. | `play/spawn.rs` |
| D5 | **Equal-z draw order is nondeterministic** — `sort_unstable_by_key` over HashMap iteration order. Reads as flicker; breaks replay. | `play.rs` |
| D6 | **The editor obeys the project's `gameplay_loop`.** A TurnBased project changes how the *editor* updates. The editor should have no time model. | `main.rs`, `engine.rs` |
| D7 | **Turn mode integrates physics with `dt = 1.0`** — one full second of velocity per turn, unrelated to any scheduler. **Fixed in Phase 5 Step 5f** (docs/ember2d-phase5-plan.md, alongside the `TurnScheduler`/`on_turn` work) — `sim::step` no longer calls `World::integrate_physics` at all in turn-based mode, on any step, not even one that resolved a turn; only realtime mode (`!gate_late_phase_on_turn`) integrates now. Nothing in the roguelike ever used a nonzero velocity anyway (every actor moves via `set_position`), so this was dead weight rather than a live bug — fixed as part of retiring the old `trigger_turn`-gated late phase for the scheduler-driven one, not because anything depended on it. | `sim.rs` |
| D8 | Hot-reload of one script clears **all** entity scopes. | `scripting/engine.rs` |
| D9 | A script that errors keeps being called and failing every frame; only the log is suppressed. | `scripting/engine.rs` |
| D10 | `spawn_entity` hardcodes white, z=2, and a 1×1 trigger collider. | `scripting/api.rs` |
| D11 | **Allocation churn per frame:** `globals.clone()` + `persistent.clone()` twice; a ~12-HashMap `ScriptState` snapshot up to 3×; every collider's `layer: String` and `mask: Vec<String>` cloned in both `ScriptState::from_world` and `detect_collisions`; timers scanned by string prefix with `.replace()` per entity per pass. **Partially mitigated in Phase 5 Step 5f** (docs/ember2d-phase5-plan.md) — found live: the user reported floor2 (~3x floor1's entity/collider count) dropping to ~21 fps in a debug build, after Step 5e/5f's `on_input`/`on_turn` passes had turned "one `ScriptState` snapshot per step" into up to three. The `World`-derived read-only maps (`positions`, `colliders`, `tags`, etc.) were pulled into a new `WorldSnapshot` type, built once per step in `PlayState::update` and shared via `Rc::clone` (O(1)) across `on_input`/`on_update`/`on_turn` instead of each rebuilding its own. Release-mode benchmarking (`TurnHarness`, idle floor2 steps) measured ~4.4ms/step with the regression (2-3 rebuilds), ~3.4ms/step with this fix (1 rebuild plus the now-cheap `Rc::clone`s), and ~2.6ms/step for a synthetic "no `on_input`/`on_turn` at all" stand-in for the pre-5e baseline — i.e. the fix lands within ~30% of that baseline, versus ~70% over it before. Debug-mode numbers (what the user actually saw) landed even closer: ~19.3ms/step fixed vs. ~18.8ms/step for that same pre-5e stand-in, a ~3% gap. This closes the specific regression Step 5e/5f introduced; it does **not** close D11 itself — the underlying per-step snapshot rebuild (now happening once instead of up to three times), the `globals`/`persistent` double-clone, and the collider `String`/`Vec<String>` clones are all still there, still `O(entities)`.

**The actual dominant cost, found immediately after** (same live investigation, still ~21 fps after the `WorldSnapshot` fix above): the project's `Cargo.toml` had no `[profile.dev]` overrides at all, so `wgpu` (and its whole dependency tree — `wgpu-core`, `wgpu-hal`, `naga`) and every other dependency compiled fully unoptimized in a `cargo run` debug build — a well-known, widely-documented source of severe CPU-side slowness for wgpu specifically (its resource/hazard tracking and command encoding are disproportionately expensive unoptimized), independent of anything this codebase does. Confirmed empirically: adding `[profile.dev.package."*"] opt-level = 3` (dependencies only — Cargo never lets this reach the workspace's own root crate) produced *zero* change in headless `TurnHarness` benchmarking, proving the remaining cost was in `ember2d`'s own code, not a dependency; adding `[profile.dev] opt-level = 1` for the crate itself (the standard companion setting — see e.g. Bevy's recommended Cargo config) cut floor2's per-step script-logic cost from ~19.1ms to ~5.1ms in the same benchmark, a ~3.75x speedup, and cut the *entire test suite's* wall-clock time by roughly the same factor. This is likely the change that actually resolves what the user experienced as "~21 fps on an RTX 2080 / i7-9700K" — the `WorldSnapshot` fix above was real and worth keeping, but was addressing a comparatively minor fraction of the total cost next to running every dependency (most of all `wgpu`) fully unoptimized. Full D11 (the remaining `O(entities)` cost *within* `ember2d`'s own optimized-at-level-1 code — borrow instead of snapshot-and-clone, intern/index globals) remains Phase 6 scope. **Closed in Phase 6** (docs/ember2d-phase6-plan.md Steps 3-8) — "borrow instead of snapshot" turned out architecturally impossible (Rhai requires registered types be `'static`, ruling out a lifetime-borrowing `ScriptState`; see that plan's §0), so the actual work was making the owned snapshot cheap: `mem::take`/put-back for globals/clips/persistent (Step 3), an `Rc<str>`-sharing/pre-sized `WorldSnapshot` (Step 4), skipping the snapshot entirely when no collision that step involves a scripted entity (Step 5), and a bitmask/sweep-and-prune rewrite of `detect_collisions` (Steps 7-8). Measured end-to-end on `roguelike/floor2.level` (release, this machine): p50 ms/step **9.723 → 1.817 (-81%)**, allocs/step **35,299 → 6,598 (-81%)** — full per-step numbers for every intermediate step, plus the synthetic n=500..10000 sweep, are in the phase plan's own §1/§3-§9 tables. The phase's stated done-when (allocs/step stops scaling with entity count) is not fully reached — `WorldSnapshot::build` is still `O(entities)` by nature, just far cheaper per entity now — but the O(colliders²) term that dominated at scale is gone. | `scripting/engine.rs`, `scripting/state.rs`, `play.rs`, `world.rs`, `Cargo.toml` |
| D12 | `layer != "locked"` — a magic string gating level exits. | `play.rs` |
| D13 | Texture sprites bypass viewport culling (`continue` before the bounds check). | `play.rs` |
| D14 | Colour-space mismatch between font atlas and textures (§2). | `renderer/backend.rs` |
| D15 | **A genuine runtime error inside `on_update`/`on_start`/`on_collide` is silently swallowed if its message happens to contain "Function not found"** — the check meant to ignore "this optional lifecycle function doesn't exist" (`e.to_string().contains("Function not found")`) can't tell that apart from a real type-mismatch error from a call *inside* a function that does exist, since Rhai reports both as the same error variant with the same substring. No compile error, no log entry, no disabled script — the function just silently stops executing partway through, every call. Found and fixed in Phase 4 (not Phase 1, since nothing before Phase 4 wrote a script with a same-pass lazy-init bug that tripped it) — see `docs/HANDOFF.md`. **Fixed**: match `ErrorFunctionNotFound`'s exact payload against the specific lifecycle function name instead of a substring of the whole error's Display text. | `scripting/engine.rs` |
| D16 | **A script's `ctx.draw_hud`-drawn HUD vanished the instant the game paused.** `PlayState::render` cleared `ScriptEngine::pending_hud_draws` itself, every frame, regardless of whether a script actually ran that frame — but `PlayState::update` (and therefore the only call site that repopulates the queue) doesn't run once a `PauseMenuState` is pushed on top, while `render` still runs for every stacked state every frame. Found and fixed in Phase 4 Step 4g while deleting the two hardcoded HUD bars — see `docs/HANDOFF.md`. **Fixed**: the clear moved to the start of `ScriptEngine::run_scripts` (the one call site that only fires on a real, unpaused frame) instead of after drawing in `render`. | `play.rs`, `scripting/engine.rs` |
| D17 | **Save/load cannot round-trip per-entity script state mid-run.** `SaveState` serializes `World` + `persistent`, but not the script engine's `globals` map, and `PlayState::on_start`'s `is_loading_save` branch skips `do_on_start` entirely — it never re-runs any entity's `on_start`. Any per-entity state a script keeps in globals (the roguelike's whole combat model: `hp_<id>`, `acted_<id>`, `aware_<id>`, `atk_turn_<id>`/`atk_dmg_<id>`, plus the level-scoped `turn`/`last_resolved_turn`) is silently lost across a save/load — an enemy would reload at full hp, asleep, and out of turn sync with a `turn` counter that reset to `()`. **Fixed in Phase 5 Step 5c** (docs/ember2d-phase5-plan.md) — a prerequisite for that phase's replay test, which needs a save/load-equivalent round trip to be exact. `SaveState` now also carries `globals`/`clips` (`#[serde(default)]`, so an old save without them still loads as empty maps); `PlayState::from_save` restores both directly into the new `PlayState` instance. The `is_loading_save` path still deliberately does **not** re-run `on_start` — that was never the right fix, since several scripts' `on_start` writes are unconditional (`enemy_rat.rhai`/`enemy_boss.rhai`'s own hp lazy-init resets to full health every time it runs), so re-running it on load would silently heal every enemy back up. Regression test: `tests/save_load_globals.rs` — saves a script-set global, round-trips it through a real RON string (not an in-memory clone), and confirms `PlayState::from_save` restores it. | `save.rs`, `play.rs`, `components/animator.rs`, `app.rs` |
| D18 | **Saving a level through the editor scrambles its tile order.** `LevelGrid::tiles` is a plain `HashMap<(i32, i32, u8), TileRecord>` (never converted to `BTreeMap` — out of scope for Phase 5 Step 5b's determinism pass, docs/ember2d-phase5-plan.md, which covers sim-relevant stores only, not the editor's own working copy). `LevelGrid::to_level_data()` builds the saved `LevelData.tiles` Vec via `self.tiles.values().cloned().collect()` — a bare HashMap-order collect, no sort — so `EditorState::save()` writes tiles out in whatever per-process-random order the hash state produces. Found live during Phase 5 (a level opened and saved in the editor while reproducing an unrelated visual report failed `tests/roguelike_level_integrity.rs`'s tile-sort check afterward — see Step 5d's report). Two real consequences, not just a cosmetic diff: (1) it breaks the `(layer, y, x)`-sorted invariant `examples/gen_roguelike.rs`'s generator establishes and that integrity test enforces, corrupting the diff of any level re-saved through the editor even with zero actual edits; (2) since `PlayState::do_on_start` spawns entities in `LevelData.tiles` order and `World::spawn()` assigns ids sequentially, a scrambled save also reassigns *which numeric EntityId lands on which tile* — re-saving the same level twice can hand a given tile two different ids across the two saves. `examples/gen_roguelike.rs`'s own header comment already named this exact hazard as the reason the generator does its own explicit sort before writing, but it was never elevated to a tracked defect until now. **Not fixed** — logged per the user's request; not currently scheduled to a phase (Phase 5's scope is the sim, not the editor; `LevelGrid` is Phase 7 territory, though the fix itself — `HashMap` → `BTreeMap`, matching Step 5b's approach for `World` — would be small and low-risk whenever it's picked up). Until then: re-run `cargo run --example gen_roguelike` after any editor session that opened and saved a `roguelike/*.level` file, and treat an editor-saved level's tile order as **not** meaningful for diffing. | `editor/grid.rs` |
| D19 | **A player keypress made while an enemy's move animation is playing is silently dropped, not delayed.** `PlayState::update`'s Phase 5.5 Part 3 animation gate (`docs/ember2d-phase5.5-plan.md`) skips calling `Simulation::step` at all while `self.animations` is non-empty — but `ember2d::sim::step` (the untouched, generic per-frame pump, `sim.rs`) already called `input`/`mouse`/`gamepad`'s `consume_step()` *before* invoking `PlayState::update` for that frame, unconditionally. `consume_step()` is what actually claims a buffered press and clears it from the buffer (`InputManager::pending`) — whether or not anything downstream goes on to read it. Since the animation-blocked branch never called `input.snapshot()`, a press claimed during that window was discarded rather than surviving to the next real step, breaking the input-buffering contract's own stated guarantee ("a press is never lost to a frame that ran zero steps," §4.1) for a case that guarantee's original design didn't anticipate: a frame where a step *runs* but internally chooses not to consume input. Found live: reported as "player movement doesn't feel great when the enemy is moving." **Fixed** in Phase 6 (out of that phase's own planned scope, landed as a standalone correctness fix): `PlayState` now reads `input`/`mouse`/`gamepad`'s snapshots every frame even while blocked, folding any `pressed` sets into a small buffer (`buffered_pressed`/`buffered_mouse_pressed`/`buffered_gamepad_pressed`) merged into the next real step's snapshot once the queue drains, instead of losing them. Only `pressed` needs carrying forward — `held` is live physical state, always correct fresh on whichever frame finally reads it. Compounding factor also addressed: `enemy_rat.rhai`/`enemy_boss.rhai`'s `animate_move` duration dropped from 0.15s to 0.08s, since several enemies acting in one round each pay their own animation's duration serially (the scheduler blocks all stepping, including the player's own next turn, until each one's animation finishes) — 3 rats at 0.15s was 450ms of the player's input going nowhere even after the drop bug itself was fixed. | `play.rs`, `roguelike/scripts/enemy_rat.rhai`, `roguelike/scripts/enemy_boss.rhai` |
| D20 | **Several actors acting in one round each pay their own animation's duration serially, stacking into one long input freeze.** `PlayState::update`'s animation gate (the same one D19 fixed the drop half of) blocked `Simulation::step` from running at all while `self.animations` — the WHOLE queue, any entity — was non-empty, not just the specific actor about to act next. On floor2 (3 rats), once all three are awake and chasing, every player move triggers 3 sequential `animate_move`s (0.08s each after D19's own duration cut) that must each fully drain, one after another, before the player regains control — up to ~240ms of total input freeze per round, scaling linearly with active enemy count. Found live: reported as "movement in the roguelike demo does feel bad again whenever taking turns with the enemy," immediately after Phase 6 Step 7 landed — verified NOT a Phase 6 regression (`play.rs`, `scheduler.rs`, and the enemy scripts were all byte-identical to their D19-fixed state; the mechanism was present, and partially mitigated, since D19 itself). **Fixed**: the gate is per-actor now, not global — `PlayState::update` checks whether the scheduler's *current front actor specifically* (`Simulation::current_actor()`) has an animation of its own still in flight, and only blocks that one entity's next turn. A different actor's turn resolves immediately regardless of what else is still playing, so N enemies' animations overlap in real time instead of stacking — worst case drops from N×duration to ~1×duration. The safety invariant the old global gate existed to protect (an actor can't receive a second `animate_move` before its first one finishes — see D19's own fix, which predates this one, for why: `current_actor()` only changes to a new actor once the current one's turn has fully resolved, so "is the actor now at the front still animating" is exactly "has THIS actor's own most recent animation finished") is preserved exactly, verified by a dedicated regression test (`tests/turn_animation.rs::two_actors_animations_overlap_instead_of_stacking`) confirmed to fail under the old global gate before being confirmed to pass under the new one. The player's own movement stays deliberately un-animated (per D19's fix), so the player is now never gated by animation state at all. | `play.rs`, `tests/turn_animation.rs`, `docs/ember2d-scripting-api.md` |
| D21 | **`check_hot_reload` issued one `fs::metadata` syscall per cached script path, every single simulation step**, regardless of whether anything on disk could plausibly have changed in under a frame — 4 syscalls/step at floor2 scale (`player`/`enemy_rat`/`pickup`/`stairs.rhai`), and a guaranteed-failing call for every node-graph-authored script (`__script_<id>` synthetic keys are never backed by a real file, so `fs::metadata` against one always errors). Found while investigating Phase 6's per-step cost breakdown (docs/ember2d-phase6-plan.md §1: "~40% of floor2's per-step cost is neither snapshot build nor collision detection" — hot-reload polling was part of that remainder). **Fixed in Phase 6 Step 6** — throttled to once every 30 calls to `run_scripts` (a plain step counter, not `Instant::now()`, since wall-clock time in `ember2d-sim` would itself be a determinism violation) and `__script_<id>` keys are now skipped outright rather than polled and left to fail. A flat 30× reduction in that syscall's frequency; imperceptible cost to the dev-time hot-reload workflow (up to 0.5s to notice a live edit instead of the next step). Not measurable via `bench_sim`'s allocation counter (`fs::metadata` is an OS call, not a heap allocation) — verified by a dedicated test (`check_hot_reload_only_runs_once_every_throttle_interval`) instead. | `scripting/engine.rs` |
| D22 | **A cancelled timer and a just-fired one are indistinguishable in storage, so `timer_done` keeps reporting "done" long after it should have consumed itself.** `ScriptCtx::cancel_timer` and the "just consumed" write `ScriptCtx::timer_done` queues when it fires (`-999.0`, meant purely as an internal marker) both resolve, in `apply_ctx`'s pending-timer write, to the exact same stored value: `-1.0`. `timer_done`'s own guard (`val <= 0.0 && val > -500.0`) treats -1.0 as "still within the done window," so the very next check after either event reports `true` again — not "once," as `docs/ember2d-scripting-api.md` documents — and keeps reporting `true` on every subsequent check until enough real simulation steps (`run_scripts`'s per-step decay) carry it past -500.0, roughly 8 minutes at 60 steps/second. Found while moving timer storage off Rhai `Scope` variables in Phase 6 Step 9 (docs/ember2d-phase6-plan.md), tracing the exact sentinel path for the first time rather than assuming the two write sites were already distinct. **Not fixed — logged per the plan's explicit instruction** (this is a behavior question, not the storage-migration Step 9 was scoped to): no shipped script (`roguelike/`, `shooter/`) calls `start_timer`/`timer_done`/`cancel_timer` at all, so nothing observable is broken today. A real fix needs its own distinct "consumed" sentinel, separate from whatever value `cancel_timer` writes. | `ember2d-sim/src/scripting/api.rs`, `ember2d-sim/src/scripting/apply.rs` |

---

## 4. Target architecture

### 4.1 Input buffering — resolution to D1

**The problem is two-sided.** Edge-triggered input (`just_pressed`) is produced per *frame*, but consumed per *simulation step*, and those cadences don't match. On a heavy frame the accumulator runs several steps and every one sees the same press. On a light frame it runs zero steps and the press is cleared by the next `poll_events` without any step observing it. Duplicates and silent drops, from the same root cause.

**Approach chosen: buffer until consumed.**

A press enters a pending set with a short lifetime. The first simulation step to run consumes it and clears it. If no step runs that frame, it survives into the next. Continuous state (`is_held`) is untouched — it's correct at any cadence and needs no buffering.

```rust
struct BufferedInput {
    pending: HashMap<Key, f32>,   // key → seconds remaining in the buffer
}
// poll_events:  insert/refresh pressed keys with BUFFER_WINDOW
// each sim step: consume → the step's just_pressed set, then remove
// each frame:    decay remaining entries, drop expired
```

`BUFFER_WINDOW` around 100–150ms. Same treatment for mouse buttons and gamepad buttons.

**Why this over the alternatives.** Unity and Bevy split the reads — edge input is only valid in the per-frame update, and you latch a bool for the fixed step. That would mean splitting script callbacks into per-frame and per-step variants: a large API change for a bug fix. Godot tags each press with the frame index for both process and physics counters, which solves duplicates cleanly but not drops. Buffering fixes both.

**Two things it buys beyond correctness:**

- A buffer window *is* jump buffering and coyote time — the input forgiveness players feel as responsiveness in platformers and action games. You get real game feel from the bug fix.
- **Turn-based mode needs it.** A keypress may have to wait several frames for the player's turn to come around. Without a buffer, turn-based input would feel like it drops presses constantly.

**Documented semantics for scripts:** `just_pressed(key)` is true in exactly one simulation step per physical press, no matter how many steps run that frame, and no press is lost to a frame that ran zero steps. Write this in the API spec.

**Edge case to handle:** a press and release inside the same frame must still register. Since the buffer records the press independently of `is_held`, that works — but test it, because it's the case a naive implementation misses.


```
Game code / Editor / Scripts
        │  emit draw commands in WORLD or SCREEN space
        ▼
   DrawList  ── sort by (space, layer, z, texture) → batch
        ▼
   Camera (world→screen)      ← new
        ▼
   RenderBackend  ── WgpuBackend (exists, extended)
```

### The command

```rust
pub struct DrawCommand {
    pub texture: TextureId,
    pub src:     Rect,      // pixels in source texture
    pub dest:    Rect,      // world units, or screen pixels if space == Screen
    pub rotation: f32,
    pub tint:    Color32,
    pub layer:   i32,
    pub z:       f32,
    pub space:   Space,     // World | Screen
}
```

`SpriteInstance` in `backend.rs` is already 90% of this — it has position, size, uv_offset, uv_size, two colours and a mode flag. What's missing is rotation, and the fact that positions arrive in cells rather than world units. **This is an extension of existing code, not a rewrite.**

### Coordinate spaces — write these down before coding

| Space | Unit | Used by |
|---|---|---|
| World | float units | entities, tiles, colliders |
| Camera | world + position/zoom | one per viewport |
| Screen | physical pixels | final output |
| UI | logical pixels, DPI-scaled | editor chrome, HUD |

In an ASCII project one world unit is one cell and zoom is constrained to integers so glyphs stay crisp. That is a **project setting**, not an engine mode.

### The cell-height question

`font8x8` is genuinely 8×8 — square. `CELL_H = 16` doubles it. On the GPU this is now a UV stretch rather than a CPU blit trick, and apparent size is a camera concern. **Rasterize at true 8×8 and let world units be square**, so physics behaves identically on both axes. This matters the moment anyone builds a platformer.

### Sprite

```rust
pub struct Sprite {
    pub source:  SpriteSource,
    pub tint:    Color32,
    pub size:    Option<Vec2>,   // None = natural size via pixels-per-unit
    pub layer:   i32,
    pub visible: bool,
}

pub enum SpriteSource {
    Texture { id: TextureId, src: Option<Rect> },
    Glyph   { ch: char, bg: Option<Color32> },
    Clip    { id: ClipId },
}
```

Unify at the render layer, keep meaning at the data layer. A bare `TextureId` + `Rect` would make the inspector show atlas pixel offsets, store those offsets in level files (so changing fonts corrupts levels), and make `set_glyph` impossible.

### Animation

Split the clip (authored, shared) from playback state (per-entity):

```rust
pub struct AnimationClip { pub frames: ClipFrames, pub fps: f32, pub looping: bool }
pub enum ClipFrames {
    Rects  { texture: TextureId, frames: Vec<Rect> },
    Glyphs { frames: Vec<char> },      // torch flicker, spinning coins
}
pub struct Animator { pub clip: ClipId, pub frame: usize, pub elapsed: f32, pub playing: bool, pub speed: f32 }
```

The existing `frames: Vec<char>` / `frame_rate` / `frame_timer` on `Sprite` migrates into `ClipFrames::Glyphs` + `Animator`. Static tiles — most of a tilemap — then carry no animation state at all.

### Turn scheduling

```rust
pub enum GameplayLoop { RealTime { }, TurnBased { model: TurnModel } }
pub enum TurnModel { Alternating, Energy, ActionCost, Declared }
```

One priority queue keyed by time: an actor acts, the action costs time, the actor is reinserted at `now + cost`. `Alternating` is that queue with every speed and cost at 100. Ship only `Alternating` exposed; define the rest.

Three structural requirements, and they are the actual work:
1. **The scheduler suspends** on a player-controlled actor and waits for a command while rendering continues.
2. **Animation is separate from simulation.** A turn resolves instantly in the sim; actions emit animation events that play over real time while the sim waits. Skip this and turn-based feels broken. ✅ **Closed in Phase 5.5** (docs/ember2d-phase5.5-plan.md Part 3), after being explicitly deferred out of Phase 5's own scope (§1.2 of that phase's plan) with the roguelike's instant-snap movement judged acceptable for that phase alone. `Simulation::step`/`late_step` emit `AnimationEvent::Move`/`Flash`/`Shake` into `StepOutcome.animations`; `ember2d::play::PlayState` owns real-time playback in a new `animations: Vec<PlayingAnimation>` queue (`play/animation.rs`) and — this is the load-bearing part — skips calling `Simulation::step` at all while that queue is non-empty, so the next turn genuinely cannot resolve underneath a still-playing animation. Playback never touches `World`/simulation state (position/tint overrides are computed at render time only), so this changes nothing about determinism — `tests/replay.rs` (which drives `Simulation` directly, bypassing `PlayState` and therefore playback entirely) still passes unchanged. `roguelike/scripts/enemy_rat.rhai` and `enemy_boss.rhai` were updated to call `ctx.animate_move` alongside their existing `ctx.set_position`, so an enemy visibly slides one cell instead of teleporting — the player's own movement was deliberately left un-animated, to avoid adding input latency to something that already felt instant and responsive.
   - **This is also why Phase 5.5 dropped an earlier draft's alternative fix** (un-gating `World::integrate_physics` so it ran every real frame in turn-based mode too): that would have made distance-travelled-per-turn a function of framerate, a real cross-platform desync risk for Phase 9's lockstep — see docs/ember2d-phase5.5-plan.md §0.2 for the full reasoning. Turn-based mode still never integrates physics at all (D7 stays fixed as Step 5f left it); if a future project genuinely needs simulated velocity between turns, the animation queue is not that — integrate a fixed amount **per turn**, not per frame, and treat it as new work when a real project asks for it.
3. **`trigger_turn` today is a global flag**, not a per-actor scheduler. It's a placeholder, not a foundation.

---

## 5. Networking

**Target: 2-player online.** That's the friendliest case in networking — one player hosts, the other connects. No dedicated server, no matchmaking, no interest management, no server costs. Most netcode difficulty scales with player count, and at two it stays small.

### 5.1 Two models, and the turn-based one is nearly free

**Turn-based (tactical RPG, turn-based RPG).** A turn is a committed command batch. Send the batch, apply it on both sides. Latency doesn't matter because nobody is waiting on frame-perfect timing. Once Phase 5's command layer exists, this is a small amount of additional work.

**Realtime (platformer, action).** A remote player's input arrives 30–80ms late and you can't wait for it. The standard answer at 2P is **rollback**: predict the remote input, simulate forward, and when the real input arrives, restore to that frame and re-simulate. It's what fighting games use and it's specifically strong at two players.

**Sequencing consequence:** ship networked turn-based first. It validates the whole stack — transport, command serialization, determinism — on the easy case, and it's the model your tactical RPG wants anyway.

### 5.2 Determinism requirements

Both models require both machines to produce identical state from identical inputs. Three current hazards:

**H1 — Nondeterministic iteration order.** `detect_collisions` builds its collidable list by iterating `HashMap`, whose order varies *between processes*. Two machines with identical inputs would emit collision events in different orders, resolve them differently, and desync within seconds. **Every iteration in the sim must be deterministic** — `BTreeMap`, or collect-and-sort by `EntityId`. This is the most likely cause of a desync you'd otherwise spend a week chasing.

**H2 — Transcendental math.** IEEE 754 makes `+ - * /` and `sqrt` reproducible across platforms; `sin`, `cos`, `atan2`, `exp`, and `powf` are **not** — they come from platform libm and differ. Current offenders: `get_angle_to` (`atan2`), `get_distance` (`sqrt`, safe), the camera lerp (`exp`), and `Vec2::normalized`. Options, in increasing order of effort: restrict sim math to the safe set; ship your own lookup-table trig; or move the sim to fixed-point. Decide before Phase 6.

> **Resolved in Phase 6 Step 12** (docs/ember2d-phase6-plan.md) — corrected
> here rather than rewriting the paragraph above, since half of it turned out
> wrong on inspection: **`Vec2::normalized` was never actually an
> offender.** It's `sqrt` plus two divides — both IEEE-754-exact, the same
> safe category `get_distance` was already correctly in — and exhaustive
> grep across the workspace at the time this was checked found it has **zero
> callers** anywhere, shipped or otherwise. The "decide before Phase 6"
> question this section posed turned out to be much smaller than framed: of
> the four named "offenders," exactly one — `get_angle_to`'s `atan2` — was
> real, and nothing else in `ember2d-sim` calls a transcendental function at
> all. Chose the first, cheapest option this section lists ("restrict sim
> math to the safe set"), not a lookup table (no table to store, version, or
> keep in sync with a fixed radius/precision) or fixed-point (would touch
> far more than one call site for a problem that turned out to be one
> function): `get_angle_to` now calls `crate::math::atan2_approx`, a minimax
> rational approximation built entirely from `+ - * /` — see that function's
> own doc comment (`ember2d-sim/src/math.rs`) for the full derivation and its
> accuracy bound (~0.01 rad). The camera lerp's `exp()` is deliberately
> **not** touched — it's presentation state (`ember2d::play::PlayState`,
> outside `ember2d-sim` entirely), already documented as not replay-safe for
> exactly this reason, and reading it back into anything a script or the sim
> depends on remains the actual thing to avoid, not the `exp()` call itself.
> Zero API break: `get_angle_to`'s signature, units, and `API_VERSION` (still
> `6`) are all unchanged — see `docs/ember2d-scripting-api.md` §6.

**H3 — Ambient randomness and time.** Already tracked as D3. The RNG must be world-owned and seeded, and both machines must start from the same seed.

**Determinism is testable without a network.** The replay test from Phase 5 — same commands plus same seed produces identical state — is exactly the desync test. Run it in CI.

### 5.3 Snapshot performance

Rollback saves and restores world state every frame. `SaveState` serializes through RON, which is orders of magnitude too slow for that. You need a **binary snapshot path** — `bincode` or hand-rolled — alongside the human-readable save format. Target: sub-millisecond for a few thousand entities. Turn-based doesn't need this; rollback can't work without it.

**Deferred out of Phase 6** (docs/ember2d-phase6-plan.md §0): `rhai::Dynamic`'s `Deserialize` implementation is `deserialize_any` (rhai 1.24's `src/serde/deserialize.rs`), which non-self-describing binary formats like `bincode` cannot serve — a real implementation needs a hand-written tagged value enum for `globals`/`persistent` first, not just a format swap. Its only consumer, Phase 9c rollback, is itself explicitly conditional in this plan ("only if the platformer needs online play"), so there was nothing forcing this to land now.

### 5.4 The five seams

Structural constraints that make netcode possible later. All of them improve save/load, replay, and testing regardless.

1. **Sim steppable headless.** `step(dt, &[Command])` with no renderer, window, or input dependency. ✅ **Closed in Phase 5.5** (docs/ember2d-phase5.5-plan.md Part 2): `ember2d_sim::simulation::Simulation` is the real thing this bullet asked for — its `step`/`late_step`/`on_start` take only device-free types (`InputSnapshot`/`MouseSnapshot`/`GamepadSnapshot`, `Command`), with zero `InputManager`/`MouseState`/`GamepadState`/winit/gilrs anywhere in `ember2d-sim`'s dependency tree (verified via `cargo tree -p ember2d-sim`). Phase 5's own `sim.rs` (in `ember2d`) is *not* this seam — its header comment said so at the time ("scaffolding... not that seam itself") — and stays exactly as it was: the generic per-frame pump shared by `PlayState`/the editor/the start screen, which genuinely need raw device access `Simulation` correctly has no knowledge of. `tests/common/mod.rs`'s `TurnHarness` was rewritten to drive `Simulation` directly, dropping every device type it used to touch.
2. **Input becomes commands, per actor.** Today scripts read the keyboard and write velocity in one breath, and the engine assumes a single player (`player_id`, one `PlayerRecord`, one `spawn_point`, one `camera_entity`). Commands must carry which actor they belong to. This is the same change that makes local co-op work. ✅ **Closed in Phase 5.5** (docs/ember2d-phase5.5-plan.md Part 2): `Simulation::step`'s `StepInput::external_commands` is a real, tested injection point — a caller other than a script's own `on_input`/`ctx.submit()` can hand a step a `Command` for any actor, merged in at the same point an internally-submitted one would land. `tests/external_commands.rs` proves it end to end (an injected "move" command resolves a turn with no key ever pressed; a command addressed to the wrong actor is correctly ignored). Commands stay script-interpreted (`action: String`, never a typed `CommandKind` enum) — the already-settled §1.1 decision in the phase 5 plan, unchanged by this. `PlayState::player_id` pluralization (Step 5g) already covered the rest of this bullet's "single player" concern.
3. **World fully serializes.** ✅ Already done via `SaveState` — needs a binary path added (§5.3).
4. **Entity IDs survive multiple authorities.** `EntityId` is `u64` monotonic with no generation. Move to `{ index: u32, generation: u32 }`, and allocate spawned IDs from authority-prefixed ranges so host and client can't collide. **Deferred out of Phase 6** (docs/ember2d-phase6-plan.md §0): ~79 `as i64` boundary casts, ~17 script sites doing `"hp_" + id` string concatenation, the `-1` no-entity sentinel, and two competing id allocators (`World::spawn` and the script-side `next_spawn_id`) would all need to change, and it breaks every existing save file — for zero benefit until Phase 9's netcode actually needs authority-prefixed ranges.
5. **Randomness and time route through the sim.** Currently violated three ways (D3).

### 5.5 Enforcement

These must survive months of single-player work that never rewards keeping them. Make the compiler do it — split into a workspace:

```
ember2d-sim/     world, components, commands, step(), rng, serialization   (no renderer/window/input)
ember2d/         engine loop, renderer, wgpu, winit, audio  → depends on sim
ember2d-editor/  editor, viewport, chrome                   → depends on both
```

Constraint 1 becomes a build error rather than a guideline. Second benefit, felt sooner: a sim crate with no window dependency is **testable**, and you currently have zero tests largely because everything needs a display.

### 5.6 Transport, and the thing that's actually hardest

**Not a plugin.** Rust has no stable ABI. Use a Cargo feature plus a `Transport` trait.

```rust
trait Transport {
    fn send(&mut self, to: PeerId, msg: &[u8]);
    fn poll(&mut self) -> Vec<(PeerId, Vec<u8>)>;
}
```

**Build `LoopbackTransport` first.** Two simulations in one process, with configurable artificial latency, jitter, and packet loss. Essentially all netcode development and debugging happens here, on one machine, before a socket exists. This is the single highest-leverage decision in the networking work.

**NAT traversal is usually harder than the netcode.** Two players behind home routers cannot simply connect. Realistic options: Steam networking (free relay and punch-through, but ties you to Steam), a WebRTC crate such as `matchbox`, or running your own relay. Budget real time here — it surprises people who assumed the simulation was the hard part.

---

## 6. Editor UI — the open decision

**The viewport is hand-written in every option.** Tile grid, camera, selection, painting, picking, gizmos — built on the engine's own renderer. That's where the engine-specific learning is, and it is never outsourced.

The question is only **chrome**: menu bar, dockable panels, text fields, file dialogs, scroll areas, DPI.

The picture has changed since the last version of this plan. You have already built a lot of chrome that works — panels, context menus, modals, a colour picker, a script editor. The old argument ("egui deletes the thing that burned you out") is weaker now, because much of it is done and the `Issues.txt` list it was aimed at is largely resolved.

**Revised recommendation: keep your own chrome, but put it behind an `EditorUi` abstraction during Phase 7.** Reasons: the sunk work is real and functional; egui would mean discarding a working script editor and palette editor; and your roadmap's V0.5.3–V0.5.9 items are exactly chrome polish you seem to want to do. The abstraction keeps the escape hatch open if panel layout becomes a sink again.

**The risk to watch:** the original burnout came from this exact work. If Phase 7 starts consuming sessions without visible progress, that's the signal to reconsider, not a reason to push harder.

---

## 7. Phases

Each phase ends with the engine **running**. Tag a release at each boundary.

### Phase 0 — Consolidate and baseline
**Nothing else starts until this is done.**

- **Decide which branch is trunk.** `gemini` is v0.5.0; `main` is v0.3.4. Merge `gemini` into `main`, or rename. Running a multi-phase refactor with a stale default branch will cause a lost-work incident.
- **Recover the demo content.** `demo/` — `level1/2/3.level` and the four `.rhai` scripts — exists **only on `main`**. `gemini` has just `tesst/`, whose `project.ron` points at a nonexistent `main.level`, plus an empty `a.rhai`. Port `demo/` forward and confirm it loads under v0.5.0's format. Without it there is nothing to regression-test.
- Run `ember2d-regression-checklist.md` against the current build; mark every item.
- Record a screen capture of the editor working.
- Tag `v0.5.0-pre-refactor`.
- Move §3 defects into GitHub issues.

**Done when:** one trunk branch, demo levels load and play, checklist marked, tag pushed.

---

### Phase 1 — Defect sweep
Low risk, no architecture change, immediately makes everything feel more solid. Also a gentle re-entry after time away.

- **D1 input buffering** — the important one. Implement buffer-until-consumed per §4.1: pending set with a decay window, consumed by the first sim step, surviving frames that run zero steps. Covers keyboard, mouse, and gamepad. Test the press-and-release-within-one-frame case.
- **D6** — the editor stops obeying `gameplay_loop`. Editor always runs realtime.
- **D2** persistent-in-`on_start`; **D4** trigger layer default; **D5** stable draw order via `EntityId` tiebreak; **D13** texture culling; **D8** per-script scope clearing; **D9** disable failed scripts; **D10** `spawn_entity` parameters; **D12** replace `"locked"` with a real flag.
- **D3 RNG** — seed from the level/world, remove the two per-frame `from_entropy()` allocations. This is also §5 constraint 5.
- **D14** colour space — pick one format and convert at load.

**Done when:** every §3 defect except D7 and D11 is closed, checklist passes.

---

### Phase 2 — World space and camera
The core of the refactor, and the thing standing between you and real 2D.

- `DrawCommand`, `DrawList`, `Space`, `Camera { position, zoom, viewport }`.
- Extend `SpriteInstance` with rotation; extend the shader.
- Renderer gains world-space entry points; existing `draw_char(cell, cell)` becomes a thin screen-space helper so the editor keeps working unchanged.
- Sort by `(space, layer, z, texture)` **before** batching, so `ensure_batch` stops degenerating.
- Move camera out of `PlayState` into a real `Camera`; delete the integer offset subtraction and the `+1` HUD-row fudge that leaks into `get_mouse_world_y`.
- Rasterize the font atlas at true 8×8; world units become square.

**Done when:** play mode renders through the camera at arbitrary float zoom, the editor is unchanged, checklist passes.

---

### Phase 3 — Sprite and asset model
- `SpriteSource` per §4; `Sprite::glyph()` and `Sprite::texture()` constructors.
- `TextureId` handles replacing per-draw path strings; `AssetManager` returns handles.
- `AnimationClip` / `ClipFrames` / `Animator`; migrate existing `Vec<char>` animation into glyph clips.
- **Level format v2**: add a `version` field (there is none today), convert cell coords to world units, store clip references by name.
- `pixels_per_unit` in `ProjectData`.
- Migrate embedded `graph:` fields — generate the Rhai once, write it beside the level, drop the field so no level file carries editor types.

**Done when:** a glyph and a PNG sprite render in one scene with correct sizes and tints, a clip plays, v1 levels load, `World → bytes → World` still exact.

---

### Phase 4 — De-hardcode play mode
`PlayState` is still a specific game: `PLAYER_SPEED`, WASD, corridor snapping, `"item"`/`"chest"` collection, score, victory condition, `z_for_tag`, two fixed HUD bars.

Move each into scripts — `player_controller.rhai`, `collectible.rhai`, `hud.rhai`. The API to do this already exists.

- Replace `z_for_tag` with the authored `layer` field (already on `TileRecord`).
- Player collider size becomes a `PlayerRecord` field, not a hardcoded `0.75`.
- Camera follow becomes a script concern using the Phase 2 camera API.

**Done when:** `PlayState` contains no tag-specific strings, no movement code, no score; the demo plays identically from scripts.

> **Amendment (Step 4k) — what actually happened, kept as historical
> record rather than rewriting the paragraph above.** This phase's plan
> assumed retrofitting the existing six-level `demo/` with three new
> scripts (`player_controller.rhai`/`collectible.rhai`/`hud.rhai`).
> Auditing `demo/` at the start of Phase 4 found only 2 of its 6 levels had
> *any* player script attached at all, and Rhai's `no_module` feature means
> scripts can't share a controller via import — there was no clean way to
> retrofit the old content. **Decision (user-approved): archive `demo/` to
> `docs/archive/demo/` and build a new, small, turn-based roguelike
> (`roguelike/`, three floors + a victory level) from scratch instead**,
> which doubles as this refactor's first deterministic, scriptable,
> automated-testable fixture — see `docs/HANDOFF.md`'s Phase 4 section and
> the full staged design at
> `C:\Users\ronal\.claude\plans\memoized-discovering-glacier.md` for the
> complete reasoning. Consequences for the bullets above:
> - `z_for_tag` → authored `layer` field: done as planned (Step 4a).
> - Player collider size → `PlayerRecord` field: done as planned (Step 4a),
>   plus an equivalent `PlayerRecord.layer` field added later (Step 4g)
>   replacing a hardcoded `Z_PLAYER` constant the same way.
> - **Camera follow did NOT become a script concern — reversed.**
>   Follow/lerp/clamp contain no tag strings, no movement code, no score
>   (the follow target already comes from the authored `camera_follow`
>   flag) — it's data-driven engine machinery, not game-specific logic.
>   Scripting it would need two new API calls (`get_level_width/height`)
>   and would leak `exp()` (§5.2 H2's named cross-platform desync hazard)
>   into script-visible simulation state for no corresponding benefit.
>   Camera stays in Rust.
> - The actual scripts that shipped: `player.rhai`, `pickup.rhai`,
>   `stairs.rhai`, `enemy_rat.rhai`, `enemy_boss.rhai`, `victory.rhai` — a
>   different split than `player_controller`/`collectible`/`hud` above,
>   since Rhai's `no_module` limitation meant one script per *role*, shared
>   by every tile that plays that role, rather than one script per UI
>   concern.
> - **"the demo plays identically from scripts" no longer applies literally**
>   — there is no longer an "original demo" behavior to match, since `demo/`
>   was archived rather than reproduced. The done-criterion that still
>   holds and was verified (`grep` across `src/play.rs`/`src/play/spawn.rs`
>   at Step 4k): `PlayState` contains no tag-specific strings, no movement
>   code, no score. The roguelike itself was played start-to-finish by the
>   user (floor1 → floor2 → floor3 → victory) and confirmed working.

---

### Phase 5 — Simulation extraction, commands, turn scheduler
Carries the §5 seams. Same restructuring, do it together.

- **Workspace split** (§5.5) — makes the rest of this phase checkable by the compiler. **Done — Phase 5 Step 5i** (docs/ember2d-phase5-plan.md): `ember2d-sim` / `ember2d` / `ember2d-editor` / `ember2d-app`, exactly the table that step's plan sketched, with `sim.rs` staying in `ember2d` rather than `ember2d-sim` (documented deviation — see that file's own doc comment: it's written entirely in terms of engine-side types, `GameState`/`InputManager`/`MouseState`/`GamepadState`, not the deterministic step a truly sim-side version would need). `cargo build -p ember2d-sim` has zero renderer/window/input-device dependency in its tree, verified via `cargo tree`. See CLAUDE.md's "Workspace layout" for the current per-crate contents and the two prep gaps this step found (a stale `Color` import path, and `scripting` needing `MouseSnapshot`/`GamepadSnapshot`) beyond what the plan called "mechanical."
- Headless `step(dt, &[Command])`.
- **Commands are per-actor**, not global: `Command { actor: EntityId, kind: CommandKind, cost: u32 }`. This is what makes 2P possible and is the same change local co-op would need. Cost defaults to 100 and is ignored under `Alternating`; actors carry `speed`.
- **Pluralise the player.** `PlayState::player_id` singular, one `PlayerRecord`, one `spawn_point`, one `camera_entity` — all assume exactly one player. Move to a collection of player actors, each with its own input source. Falls out naturally from the command layer. **Partially done — Phase 5 Step 5g** (docs/ember2d-phase5-plan.md): `PlayState::player_id` is gone — the exit-trigger and solid-collision checks in `late_update` now ask "is this entity's `Actor::controller` a `Controller::Local`" (any match counts) instead of comparing against one stored id. Authoring (`PlayerRecord`, `spawn_point`) and `camera_entity` deliberately stay singular per that step's own scope — split-screen/local-co-op authoring is still future work, this only stopped the *code* from assuming one player exists.
- Turn scheduler as a priority queue; expose `Alternating` only. Fixes D7.
- Scheduler suspension on player-controlled actors.
- Animation event queue, decoupled from sim time.
- Interpolation between fixed steps for rendering.
- **Deterministic iteration everywhere in the sim** (§5.2 H1) — `BTreeMap` or collect-and-sort. Start here rather than retrofitting.

**Done when:** a recorded command list replayed against the same seed produces an identical world, byte for byte. That replay test is your first automated test *and* your desync test. **Met — Phase 5 Step 5h** (docs/ember2d-phase5-plan.md): `tests/replay.rs` drives two independent `TurnHarness`/`PlayState`/`World`/`ScriptEngine` stacks through the identical input sequence and asserts RON-serialized `World`+`globals`+`persistent`+`clips` are byte-identical at every checkpoint and at the end; passed 5/5 fresh-process runs. One deliberate narrowing from the literal sketch above: it replays the same recorded *key* sequence rather than literal `Command` values injected past `on_input` — building a real command-injection entry point (bypassing `on_input`) is deferred to whenever Phase 9's netcode or the Step 5i workspace split actually needs one to consume, not before; see that test file's own header comment for the full reasoning. This doesn't close the phase (5i's workspace split, the scheduler/animation/interpolation bullets above are all still open) — just its stated "done when" gate. **The command-injection entry point this paragraph deferred now exists** — see Phase 5.5 below; `replay.rs` itself was left exactly as this paragraph describes (still key-based, still proving the full pipeline), and a *separate* new test (`tests/external_commands.rs`) proves the injection point instead, deliberately not folded into this one.

---

### Phase 5.5 — Close the netcode seams, add the animation queue

Ran between Phase 5 and Phase 6, not originally planned as its own phase — see `docs/ember2d-phase5.5-plan.md` for the full plan and reasoning (including §0.2's explanation of why an earlier draft's turn-based-physics idea was dropped in favor of the animation queue). Summary of what shipped, verified against the actual code, not just the plan: §5.4 seams 1 and 2 closed (`ember2d_sim::simulation::Simulation`, `StepInput::external_commands`); the animation queue (§4's "Turn scheduling" structural requirement 2) implemented and wired into the roguelike's own enemy scripts; a GitHub Actions CI workflow added (`windows-latest` + `ubuntu-latest`, `cargo build` + `--lib` + the named integration tests, including a new `tests/external_commands.rs` and `tests/turn_animation.rs`). Explicitly out of scope, left for whenever a real project needs them: physics in turn-based mode (see the "Turn scheduling" bullet above), the `Energy`/`ActionCost`/`Declared` turn models, real transport/rollback/snapshot performance (Phase 9), and the D11 allocation work (Phase 6) — `StepOutcome`'s per-step `Vec` allocations included, flagged inline in `simulation.rs` for whoever picks that up.

---

### Phase 6 — Performance and data-model hardening
- **D11** — kill the per-frame clone churn. Snapshot only what scripts read; borrow instead of clone; intern or index globals.
- **Collision layers become a bitmask** with names in project settings. Currently `String` compares run inside `raycast` and `get_path`'s inner loops.
- `EntityId { index, generation }` (§5.4 constraint 4), with authority-prefixed allocation ranges so host and client can't collide. Also fixes stale-handle reuse.
- **Transcendental math decision** (§5.2 H2) — restrict sim math to the reproducible set, ship lookup-table trig, or move to fixed-point. `get_angle_to`, `Vec2::normalized`, and the camera lerp are the current offenders. Decide here; retrofitting later is painful.
- **Binary snapshot path** (§5.3) — `bincode` alongside RON. Target sub-millisecond for a few thousand entities. Rollback is impossible without it; save/load benefits immediately.
- Timers move off scope-variable string scanning into a real per-entity store.
- Spatial hash for collision broad-phase, only if profiling says so.
- Component registration macro so `despawn`/`entity_ids` stop needing manual edits.

**Done when:** a 2,000-entity level holds 60fps, profiling shows no per-frame allocation proportional to entity count, and a full world snapshot round-trips in under a millisecond.

**Amendment — what actually shipped** (see `docs/ember2d-phase6-plan.md` for the full 14-step account; this paragraph is the summary, not a replacement for the sketch above, per this doc's own append-don't-rewrite convention):

- **D11** — closed (Steps 3-8). "Borrow instead of snapshot" was found architecturally impossible (Rhai requires registered types be `'static`; see phase6-plan.md §0) — the shipped fix instead made the owned snapshot itself cheap: `mem::take` instead of cloning globals/clips/persistent (Step 3), an `Rc<str>`-sharing/pre-sized `WorldSnapshot` (Step 4), skipping the snapshot build entirely when a step has no scripted collision to run (Step 5), and the bitmask/sweep-and-prune collision rewrite (Steps 7-8, see below). floor2: 9.723ms → 1.817ms/step (-81%), 35,299 → 6,598 allocs/step (-81%). Full numbers above at §3's D11 entry.
- **Collision layers become a bitmask** — done as planned (Step 7), plus a project-settings `collision_layers: Vec<String>` list (registration-order bit assignment, never grown at runtime — see Step 7's own determinism reasoning) and a sweep-and-prune broad phase (Step 8) the original sketch listed as a separate, conditional item ("only if profiling says so") — profiling (§1's baseline) said so: `detect_collisions` was 65-86% of total step time at floor2/synthetic-10k scale before Step 8, 14-22% after.
- **`EntityId { index, generation }`** — deferred, not built. ~79 `as i64` boundary casts, ~17 script sites doing `"hp_" + id` string concatenation, the `-1` no-entity sentinel, and two competing id allocators would all need to change for zero benefit until Phase 9's netcode needs authority-prefixed ranges. See phase6-plan.md §0's deferral table.
- **Transcendental math decision** (§5.2 H2) — resolved as "restrict to the reproducible set," not lookup tables or fixed-point: exhaustive grep found exactly one real offender (`atan2` in `get_angle_to`), replaced with a hand-verified rational approximation (Step 12). `Vec2::normalized`, named as an offender in the original sketch, turned out to have zero callers and to already be IEEE-754-exact (sqrt + divides) — corrected in §5.2 below rather than touched.
- **Binary snapshot path** (§5.3) — deferred, not built. See the note added to §5.3 itself.
- **Timers move off scope-variable string scanning** — done as planned (Step 9), plus D22 found and logged (not fixed — a pre-existing behavior bug, not a regression from the migration).
- **Spatial hash for collision broad-phase** — sweep-and-prune (1D, sort-by-x) was chosen instead once profiling ran; see the collision-layers bullet above.
- **Component registration macro** — rejected, not built. Would contradict CLAUDE.md's "deliberate learning artifact" rule, and the phase's own finding (`entity_ids` unioned only 4 of 7 stores, Step 11) shows a macro wouldn't have prevented the actual bug either, since that call site had zero production callers to ever surface the drift regardless of how the store list was maintained. Fixed the omission directly instead, plus a manual six-site checklist comment on `World` itself.
- Also landed, out of the original scope entirely: a benchmark harness (`ember2d-sim/examples/bench_sim.rs`, Step 1) that made every number above possible to measure rather than estimate; two live-reported correctness fixes jumped the queue mid-phase (D19: dropped input during an animation-blocked frame; D20: the same gate's per-actor fix); a hot-reload syscall throttle (D21); a `DrawList`-buffer-reuse item (Step 13) checked and skipped once Steps 1-12's cumulative work was measured to have already hit 60fps on the target level in a debug build.

---

### Phase 7 — Editor: viewport panelization and chrome
Absorbs roadmap V0.5.3–V0.5.9.

- Viewport becomes a dockable, non-closable panel; master-fill layout (V0.5.3).
- Rulers, high-contrast selection, smooth camera — now trivial on the Phase 2 camera.
- Inspector 2.0 property grid, collapsible sections, inline widgets (V0.5.4).
- Asset preview and drag-and-drop (V0.5.5), tooltips and toasts (V0.5.6), command palette (V0.5.7), themes and layout profiles (V0.5.8), undo/redo audit and perf pass (V0.5.9).
- Chrome behind an `EditorUi` abstraction (§6).

**Done when:** the roadmap's V0.5.x items are closed and the checklist passes.

---

### Phase 8 — Asset and animation authoring
Roadmap V0.6.0.

- Tileset importer: slice a sheet into a grid, name regions, write to project assets.
- Sprite animation editor: build clips, scrub frames, preview looping.
- Both write the Phase 3 formats — which is why they come after it, not before.

---

### Phase 9 — Networked 2-player
Everything before this builds the seams; this is the first phase that ships netcode. **Can be pulled earlier — right after Phase 5 — if the tactical RPG is the game you build first.** Turn-based networking needs almost nothing from Phases 6–8.

**10a — Loopback and harness**
- `Transport` trait behind a `netcode` Cargo feature.
- `LoopbackTransport`: two sims in one process, with configurable latency, jitter, and packet loss.
- Command serialization; a `NetSession` that exchanges command batches per step.
- Desync detector: hash world state every N steps, compare, log the first divergent frame.

All of 10a runs on one machine. Most netcode debugging happens here.

**10b — Turn-based over the wire**
- Host/client roles; host is authoritative on turn order.
- Send committed command batches; both sides apply identically.
- Reconnect and resync via a full snapshot.
- **Done when:** two instances play a tactical-RPG level to completion with no desync at 150ms simulated latency.

**10c — Realtime rollback** *(only if the platformer needs online play)*
- Ring buffer of binary snapshots (§5.3).
- Predict remote input, roll back and re-simulate on arrival.
- Input delay tuning; rollback frame cap.
- **Done when:** two instances play a platformer level at 100ms latency with no visible correction under normal movement.

**10d — Real transport**
- Pick one: Steam networking, `matchbox`/WebRTC, or a self-hosted relay (§5.6).
- **Budget real time for NAT traversal** — it is usually harder than the netcode.

---

### Phase 10 — Presets and cleanup
- **Presets, not modes.** `VisualStyle` stops being a renderer switch and becomes initial project settings: zoom constraints, grid snapping, default sprite constructor, palette, tool defaults. Ship ASCII, Sprite, and Empty presets. If a preset ever needs an `if` inside the engine, it's wrong.
- Delete `rollback_position` (superseded by `resolve_solid_collision`).
- `PlayerRecord` becomes a normal entity (prefab groundwork).
- Doc comments reconciled with reality.

---

## 8. Where this conflicts with `roadmaptoV0.6.md`

Your roadmap does editor polish (V0.5.3–V0.5.9) **before** assets and animation (V0.6.0). This plan puts the renderer and sprite work first.

**The argument for renderer-first:** V0.5.3's viewport panelization, rulers, and smooth camera are all camera and coordinate-space features. Built on today's cell renderer they get rebuilt in Phase 2. Built after it, they're straightforward. Same for V0.5.5 asset preview — it previews assets that don't have a real model until Phase 3.

**The argument for your order:** editor polish is visible, satisfying, and low-risk, and momentum matters more than optimal sequencing for a solo project with a burnout history.

**Suggested compromise:** Phases 0–1 first regardless — they're cheap and they fix real bugs. Then, if you want editor work before the renderer, take V0.5.4, V0.5.6, V0.5.7, and V0.5.8 early (inspector, toasts, command palette, theming — none of them touch coordinates), and hold V0.5.3 and V0.5.5 until after Phase 2 and 3.

---

## 9. Notes for the AI implementing this

- **One phase per branch, one logical change per commit.**
- **Preserve the comment style.** This codebase is heavily commented as a deliberate learning artifact. When code changes, rewrite the comment to match — never delete it, never leave it describing behaviour that no longer exists.
- **The engine must run at the end of every phase.** If a phase can't land working, split it.
- **Run the regression checklist before declaring a phase done.** Until Phase 5 there are no automated tests.
- **Don't opportunistically refactor** outside the current phase; note it and move on.
- **Ask before changing public API shape** — scripts and level files depend on it.
- Suggested reading order: `lib.rs` → `engine.rs` → `world.rs` → `renderer/mod.rs` → `renderer/backend.rs` → `play.rs` → `scripting/api.rs` → the phase's targets.

---

## 10. Risks

| Risk | Mitigation |
|---|---|
| Branch divergence causes lost work | Phase 0, before anything else |
| No demo content on `gemini` to test against | Phase 0 ports `demo/` forward |
| Phase 2 touches every draw call | Keep `draw_char` working as a screen-space helper; editor stays untouched until Phase 7 |
| D1's fix changes script-visible input semantics | Semantics settled (§4.1) and documented in the API spec; verify against the demo scripts |
| Editor polish becomes another months-long sink | §6 — watch for sessions without visible progress |
| Netcode seams erode during single-player work | Workspace split makes violations a build error; the replay test in CI doubles as the desync test |
| Desync from nondeterministic iteration or platform libm | §5.2 — deterministic iteration from Phase 5, transcendental decision in Phase 6, hash-compare detector in Phase 9a |
| NAT traversal turns out to be the real cost | Phase 9d is scoped separately; loopback transport keeps everything else testable without it |
| Netcode competes with shipping a game | Phase 9a–b only; defer 9c rollback until a realtime game actually needs online play |
| Scope creep back to "engine does everything" | After Phase 4, let the game you're building with your friend decide what's next |

---

## 11. Open questions

1. ~~D1's rule: how should scripts see `just_pressed`?~~ **Resolved** — buffer until consumed. See §4.1.
2. What does the game with your friend need? That reorders Phases 6–10.
3. ~~Multiplayer model?~~ **Resolved** — 2-player online. Turn-based games get lockstep command exchange (Phase 9b); realtime gets rollback if and when a realtime game needs online (Phase 9c). See §5.
4. **Genre requirements not yet folded into phases.** Platformer needs acceleration and a grounded state on `Transform` (currently velocity-only Euler) plus one-way platforms, which need collision resolution to know movement direction. Tactical RPG needs weighted and 8-directional pathfinding, a "reachable within N movement" query, and the `ActionCost` or `Energy` turn model rather than `Alternating`. RPG mostly needs UI scaffolding — dialogue boxes with word wrap and scrolling text. All three want movement out of the engine, which is Phase 4.
5. Prefabs — deferred by earlier decision. `PlayerRecord` being special-cased is the same problem showing up early; Phases 5 and 10 touch it.
6. Keep `font8x8`, or move to a scalable font once the atlas is rebuilt?
